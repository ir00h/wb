use std::collections::BTreeMap;
use std::path::Path;

use ra_ap_rustc_lexer::{FrontmatterAllowed, TokenKind, tokenize};
use serde_json::Value;

use crate::finding::{self, Finding, line_of};
use crate::process;
use crate::purity::{self, SourceFile};

struct Layer {
    package: &'static str,
    role: &'static str,
    pure: bool,
    normal: &'static [&'static str],
    build: &'static [&'static str],
    dev: &'static [&'static str],
}

const LAYERS: &[Layer] = &[
    Layer {
        package: "wb-model",
        role: "pure shapes and rules",
        pure: true,
        normal: &["serde", "serde_json", "thiserror", "ts-rs"],
        build: &[],
        dev: &["serde_json"],
    },
    Layer {
        package: "wb-core",
        role: "boards on disk",
        pure: false,
        normal: &["rustix", "serde_json", "thiserror", "wb-model"],
        build: &[],
        dev: &["wb-core"],
    },
    Layer {
        package: "wb",
        role: "the command-line tool",
        pure: false,
        normal: &["clap", "serde", "serde_json", "wb-core", "wb-model"],
        build: &[],
        dev: &["serde_json"],
    },
    Layer {
        package: "whiteboard",
        role: "the desktop shell",
        pure: false,
        normal: &[
            "block2",
            "objc2",
            "objc2-foundation",
            "serde",
            "serde_json",
            "tauri",
            "tiny_http",
            "wb-core",
            "wb-model",
        ],
        build: &["tauri-build"],
        dev: &[],
    },
    Layer {
        package: "xtask",
        role: "the check command and the bindings generator",
        pure: false,
        normal: &[
            "oxc_allocator",
            "oxc_parser",
            "oxc_span",
            "proc-macro2",
            "ra-ap-rustc_lexer",
            "serde_json",
            "syn",
            "toml",
            "ts-rs",
            "wb-model",
        ],
        build: &[],
        dev: &[],
    },
];

const ROOT_MANIFEST: &str = "Cargo.toml";
const UNSAFE_CODE_HOME: &str = "crates/shell/src/automation/macos.rs";
const UNSAFE_WORDS: [&str; 2] = ["unsafe", "unsafe_code"];

pub fn packages() -> impl Iterator<Item = &'static str> {
    LAYERS.iter().map(|layer| layer.package)
}

struct Seam {
    package: &'static str,
    feature: &'static str,
    purpose: &'static str,
}

const SEAMS: &[Seam] = &[Seam {
    package: "wb-core",
    feature: "volume-seam",
    purpose: "lets a caller choose how the core writes instead of probing the volume",
}];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Normal,
    Build,
    Dev,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Normal => "normal",
            Kind::Build => "build",
            Kind::Dev => "dev",
        }
    }

    fn allowed(self, layer: &Layer) -> &'static [&'static str] {
        match self {
            Kind::Normal => layer.normal,
            Kind::Build => layer.build,
            Kind::Dev => layer.dev,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FeatureUse {
    package: String,
    feature: String,
    kind: Kind,
}

#[derive(Default)]
struct Package {
    manifest: String,
    library_root: Option<String>,
    build_script: Option<String>,
    binaries: Vec<String>,
    dependencies: Vec<(String, Kind)>,
    features_turned_on: Vec<FeatureUse>,
}

pub fn check(root: &Path) -> Result<(), String> {
    let metadata = process::output(
        root,
        &["cargo", "metadata", "--locked", "--format-version", "1"],
    )?;
    let packages = parse_metadata(&metadata, root)?;
    let tracked = finding::tracked_files(root)?;
    let mut findings = table_findings(&packages);
    findings.extend(seam_findings(&packages));
    findings.extend(nested_workspaces(root, &tracked));
    for layer in LAYERS.iter().filter(|layer| layer.pure) {
        if let Some(package) = packages.get(layer.package) {
            findings.extend(purity_findings(root, package, &tracked));
        }
    }
    for path in &tracked {
        let is_rust = Path::new(path)
            .extension()
            .is_some_and(|extension| extension == "rs");
        if !is_rust || path == UNSAFE_CODE_HOME {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(root.join(path)) {
            findings.extend(unsafe_uses(path, &text));
        }
    }
    finding::report(&findings)
}

fn table_findings(packages: &BTreeMap<String, Package>) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (name, package) in packages {
        let Some(layer) = LAYERS.iter().find(|layer| layer.package == name) else {
            findings.push(Finding::whole(
                &package.manifest,
                format!(
                    "package {name} is missing from the layer table; add it to LAYERS in crates/xtask/src/layers.rs with the dependencies it may use"
                ),
            ));
            continue;
        };
        for (dependency, kind) in &package.dependencies {
            if !kind.allowed(layer).contains(&dependency.as_str()) {
                findings.push(Finding::whole(
                    &package.manifest,
                    format!(
                        "{name} ({}) depends directly on {dependency} as a {} dependency; add it to the table in `layers.rs` if the layer may use it",
                        layer.role,
                        kind.name()
                    ),
                ));
            }
        }
    }
    findings
}

fn seam_findings(packages: &BTreeMap<String, Package>) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (name, package) in packages {
        for used in &package.features_turned_on {
            let Some(seam) = SEAMS
                .iter()
                .find(|seam| seam.package == used.package && seam.feature == used.feature)
            else {
                continue;
            };
            if name == seam.package && used.kind == Kind::Dev {
                continue;
            }
            findings.push(Finding::whole(
                &package.manifest,
                format!(
                    "{name} turns on the feature `{}` of {} as a {} dependency; only the tests of {} itself may, because it {}",
                    seam.feature,
                    seam.package,
                    used.kind.name(),
                    seam.package,
                    seam.purpose
                ),
            ));
        }
    }
    findings
}

fn unsafe_uses(path: &str, text: &str) -> Vec<Finding> {
    let mut offset = 0;
    let mut found = Vec::new();
    for token in tokenize(text, FrontmatterAllowed::No) {
        let length = token.len as usize;
        let word = &text[offset..offset + length];
        if token.kind == TokenKind::Ident && UNSAFE_WORDS.contains(&word) {
            found.push(Finding::new(
                path,
                line_of(text, offset),
                format!(
                    "`{word}` outside {UNSAFE_CODE_HOME}; only the automation channel's native web view calls may use unsafe code, so find a safe API or move the call there"
                ),
            ));
        }
        offset += length;
    }
    found
}

fn nested_workspaces(root: &Path, tracked: &[String]) -> Vec<Finding> {
    tracked
        .iter()
        .filter(|path| path.as_str() != ROOT_MANIFEST && path.ends_with("/Cargo.toml"))
        .filter_map(|path| {
            let text = std::fs::read_to_string(root.join(path)).ok()?;
            declares_workspace(&text).then(|| {
                Finding::whole(
                    path,
                    "declares its own workspace, so the layer table does not cover it; make it a member of the repository workspace",
                )
            })
        })
        .collect()
}

fn declares_workspace(manifest: &str) -> bool {
    manifest
        .parse::<toml::Table>()
        .is_ok_and(|table| table.contains_key("workspace"))
}

fn target_findings(package: &Package) -> Vec<Finding> {
    let mut findings = Vec::new();
    if let Some(build_script) = &package.build_script {
        findings.push(Finding::whole(
            build_script,
            "a pure package has no build script, because a build script runs with file, process and network access; move the work into wb-core or xtask",
        ));
    }
    if package.library_root.is_none() {
        findings.push(Finding::whole(
            &package.manifest,
            "a pure package must be a library",
        ));
    }
    findings.extend(package.binaries.iter().map(|binary| {
        Finding::whole(
            binary,
            "a pure package has no binary, because a binary builds with the standard library and its file, process and network access; move the program into wb or xtask",
        )
    }));
    findings
}

fn purity_findings(root: &Path, package: &Package, tracked: &[String]) -> Vec<Finding> {
    let mut findings = target_findings(package);
    let Some(crate_root) = &package.library_root else {
        return findings;
    };
    let folder = package.manifest.trim_end_matches(ROOT_MANIFEST);
    let test_folder = format!("{folder}tests/");
    let sources: Vec<(String, String)> = tracked
        .iter()
        .filter(|path| {
            path.starts_with(folder)
                && !path.starts_with(&test_folder)
                && Path::new(path)
                    .extension()
                    .is_some_and(|extension| extension == "rs")
        })
        .filter_map(|path| Some((path.clone(), std::fs::read_to_string(root.join(path)).ok()?)))
        .collect();
    let files: Vec<SourceFile> = sources
        .iter()
        .map(|(path, text)| SourceFile { path, text })
        .collect();
    findings.extend(purity::findings(crate_root, &files));
    findings
}

fn relative(root: &Path, path: Option<&str>) -> Option<String> {
    let path = Path::new(path?);
    Some(
        path.strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/"),
    )
}

fn kind_of(dependency: &Value) -> Kind {
    match dependency["kind"].as_str() {
        Some("build") => Kind::Build,
        Some("dev") => Kind::Dev,
        _ => Kind::Normal,
    }
}

fn package_facts(package: &Value, root: &Path) -> Package {
    let targets = package["targets"].as_array().map_or(&[][..], Vec::as_slice);
    let paths_of = |kind: &str| {
        targets
            .iter()
            .filter(|target| {
                target["kind"]
                    .as_array()
                    .is_some_and(|kinds| kinds.iter().any(|named| named.as_str() == Some(kind)))
            })
            .filter_map(|target| relative(root, target["src_path"].as_str()))
            .collect::<Vec<String>>()
    };
    let mut dependencies: Vec<(String, Kind)> = package["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|dependency| {
            Some((dependency["name"].as_str()?.to_owned(), kind_of(dependency)))
        })
        .collect();
    dependencies.sort_by(|a, b| a.0.cmp(&b.0));
    Package {
        manifest: relative(root, package["manifest_path"].as_str()).unwrap_or_default(),
        library_root: paths_of("lib").into_iter().next(),
        build_script: paths_of("custom-build").into_iter().next(),
        binaries: paths_of("bin"),
        dependencies,
        features_turned_on: features_turned_on(package),
    }
}

fn features_turned_on(package: &Value) -> Vec<FeatureUse> {
    let name = package["name"].as_str().unwrap_or_default();
    let by_dependencies = package["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|dependency| {
            let kind = kind_of(dependency);
            let used = dependency["name"].as_str().unwrap_or_default();
            dependency["features"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(move |feature| FeatureUse {
                    package: used.to_owned(),
                    feature: feature.to_owned(),
                    kind,
                })
        });
    let by_own_features = package["features"]
        .as_object()
        .into_iter()
        .flatten()
        .flat_map(|(own, turned_on)| {
            turned_on
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter_map(move |entry| match entry.split_once('/') {
                    Some((used, feature)) => Some((used.trim_end_matches('?'), feature)),
                    None if own == "default" && !entry.starts_with("dep:") => Some((name, entry)),
                    None => None,
                })
                .map(|(used, feature)| FeatureUse {
                    package: used.to_owned(),
                    feature: feature.to_owned(),
                    kind: Kind::Normal,
                })
        });
    by_dependencies.chain(by_own_features).collect()
}

fn parse_metadata(json: &str, root: &Path) -> Result<BTreeMap<String, Package>, String> {
    let metadata: Value =
        serde_json::from_str(json).map_err(|error| format!("cargo metadata: {error}"))?;
    let members: Vec<&str> = metadata["workspace_members"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    Ok(metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|package| {
            package["id"]
                .as_str()
                .is_some_and(|id| members.contains(&id))
        })
        .filter_map(|package| {
            Some((
                package["name"].as_str()?.to_owned(),
                package_facts(package, root),
            ))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use std::path::Path;

    use super::{
        Kind, Package, declares_workspace, parse_metadata, seam_findings, table_findings,
        target_findings, unsafe_uses,
    };

    fn packages(entries: &[(&str, &[(&str, Kind)])]) -> BTreeMap<String, Package> {
        entries
            .iter()
            .map(|(name, dependencies)| {
                (
                    (*name).to_owned(),
                    Package {
                        manifest: format!("crates/{name}/Cargo.toml"),
                        dependencies: dependencies
                            .iter()
                            .map(|(dependency, kind)| ((*dependency).to_owned(), *kind))
                            .collect(),
                        ..Package::default()
                    },
                )
            })
            .collect()
    }

    fn messages(packages: &BTreeMap<String, Package>) -> Vec<String> {
        table_findings(packages)
            .into_iter()
            .map(|finding| finding.to_string())
            .collect()
    }

    #[test]
    fn the_intended_dependencies_pass() {
        let intended = packages(&[
            (
                "wb-model",
                &[
                    ("serde", Kind::Normal),
                    ("ts-rs", Kind::Normal),
                    ("serde_json", Kind::Dev),
                ],
            ),
            ("wb-core", &[("wb-model", Kind::Normal)]),
            (
                "whiteboard",
                &[("tauri", Kind::Normal), ("tauri-build", Kind::Build)],
            ),
        ]);
        assert_eq!(messages(&intended), Vec::<String>::new());
    }

    #[test]
    fn a_dependency_outside_the_table_is_named_without_a_position() {
        let found = packages(&[
            (
                "wb-model",
                &[("serde_json", Kind::Build), ("wb-core", Kind::Dev)],
            ),
            (
                "wb-core",
                &[("clap_builder", Kind::Normal), ("clap", Kind::Dev)],
            ),
            ("wb", &[("tauri-utils", Kind::Normal)]),
        ]);
        assert_eq!(
            messages(&found),
            vec![
                "crates/wb/Cargo.toml: wb (the command-line tool) depends directly on tauri-utils as a normal dependency; add it to the table in `layers.rs` if the layer may use it",
                "crates/wb-core/Cargo.toml: wb-core (boards on disk) depends directly on clap_builder as a normal dependency; add it to the table in `layers.rs` if the layer may use it",
                "crates/wb-core/Cargo.toml: wb-core (boards on disk) depends directly on clap as a dev dependency; add it to the table in `layers.rs` if the layer may use it",
                "crates/wb-model/Cargo.toml: wb-model (pure shapes and rules) depends directly on serde_json as a build dependency; add it to the table in `layers.rs` if the layer may use it",
                "crates/wb-model/Cargo.toml: wb-model (pure shapes and rules) depends directly on wb-core as a dev dependency; add it to the table in `layers.rs` if the layer may use it",
            ]
        );
    }

    #[test]
    fn the_volume_seam_is_refused_everywhere_but_in_the_tests_of_the_core_itself() {
        let metadata = r#"{
            "workspace_members": ["core", "cli", "shell"],
            "packages": [
                {
                    "id": "core",
                    "name": "wb-core",
                    "manifest_path": "/repo/crates/core/Cargo.toml",
                    "features": { "volume-seam": [] },
                    "dependencies": [
                        { "name": "wb-core", "kind": "dev", "features": ["volume-seam"] }
                    ],
                    "targets": []
                },
                {
                    "id": "cli",
                    "name": "wb",
                    "manifest_path": "/repo/crates/cli/Cargo.toml",
                    "features": {},
                    "dependencies": [
                        { "name": "wb-core", "kind": null, "features": ["volume-seam"] },
                        { "name": "wb-core", "kind": "dev", "features": ["volume-seam"] }
                    ],
                    "targets": []
                },
                {
                    "id": "shell",
                    "name": "whiteboard",
                    "manifest_path": "/repo/crates/shell/Cargo.toml",
                    "features": { "testing": ["wb-core?/volume-seam"] },
                    "dependencies": [{ "name": "wb-core", "kind": null, "features": [] }],
                    "targets": []
                }
            ]
        }"#;
        let packages = parse_metadata(metadata, Path::new("/repo")).expect("the metadata is valid");
        let found: Vec<String> = seam_findings(&packages)
            .into_iter()
            .map(|finding| finding.to_string())
            .collect();
        assert_eq!(
            found,
            vec![
                "crates/cli/Cargo.toml: wb turns on the feature `volume-seam` of wb-core as a normal dependency; only the tests of wb-core itself may, because it lets a caller choose how the core writes instead of probing the volume",
                "crates/cli/Cargo.toml: wb turns on the feature `volume-seam` of wb-core as a dev dependency; only the tests of wb-core itself may, because it lets a caller choose how the core writes instead of probing the volume",
                "crates/shell/Cargo.toml: whiteboard turns on the feature `volume-seam` of wb-core as a normal dependency; only the tests of wb-core itself may, because it lets a caller choose how the core writes instead of probing the volume",
            ]
        );
    }

    #[test]
    fn a_core_that_turns_its_volume_seam_on_by_default_is_refused() {
        let metadata = r#"{
            "workspace_members": ["core"],
            "packages": [{
                "id": "core",
                "name": "wb-core",
                "manifest_path": "/repo/crates/core/Cargo.toml",
                "features": { "default": ["volume-seam"], "volume-seam": [] },
                "dependencies": [],
                "targets": []
            }]
        }"#;
        let packages = parse_metadata(metadata, Path::new("/repo")).expect("the metadata is valid");
        assert_eq!(seam_findings(&packages).len(), 1);
    }

    #[test]
    fn a_package_missing_from_the_table_fails() {
        assert_eq!(
            messages(&packages(&[("wb-sync", &[])])),
            vec![
                "crates/wb-sync/Cargo.toml: package wb-sync is missing from the layer table; add it to LAYERS in crates/xtask/src/layers.rs with the dependencies it may use"
            ]
        );
    }

    #[test]
    fn a_pure_package_with_a_binary_target_is_reported() {
        let metadata = r#"{
            "workspace_members": ["model"],
            "packages": [{
                "id": "model",
                "name": "wb-model",
                "manifest_path": "/repo/crates/model/Cargo.toml",
                "dependencies": [],
                "targets": [
                    { "kind": ["lib"], "src_path": "/repo/crates/model/src/lib.rs" },
                    { "kind": ["bin"], "src_path": "/repo/crates/model/src/bin/dump.rs" },
                    { "kind": ["bin"], "src_path": "/repo/crates/model/tools/main.rs" },
                    { "kind": ["test"], "src_path": "/repo/crates/model/tests/notes.rs" }
                ]
            }]
        }"#;
        let packages = parse_metadata(metadata, Path::new("/repo")).expect("the metadata is valid");
        let found: Vec<String> = target_findings(&packages["wb-model"])
            .into_iter()
            .map(|finding| finding.to_string())
            .collect();
        assert_eq!(
            found,
            vec![
                "crates/model/src/bin/dump.rs: a pure package has no binary, because a binary builds with the standard library and its file, process and network access; move the program into wb or xtask",
                "crates/model/tools/main.rs: a pure package has no binary, because a binary builds with the standard library and its file, process and network access; move the program into wb or xtask",
            ]
        );
    }

    #[test]
    fn a_pure_library_without_other_targets_passes() {
        let package = Package {
            manifest: "crates/model/Cargo.toml".to_owned(),
            library_root: Some("crates/model/src/lib.rs".to_owned()),
            ..Package::default()
        };
        assert!(target_findings(&package).is_empty());
    }

    #[test]
    fn a_manifest_with_its_own_workspace_is_found() {
        assert!(declares_workspace(
            "[package]\nname = \"helper\"\n\n[workspace]\n"
        ));
        assert!(declares_workspace("workspace = { members = [] }\n"));
        assert!(!declares_workspace(
            "[package]\nname = \"wb\"\nversion.workspace = true\n\n[dependencies]\nclap.workspace = true\n"
        ));
    }

    #[test]
    fn unsafe_code_is_reported_with_its_line() {
        let text = "#![allow(unsafe_code)]\nfn read() {\n    unsafe { core::ptr::null::<u8>().read() };\n}\nconst NOTE: &str = \"unsafe\";\n";
        let lines: Vec<Option<usize>> = unsafe_uses("crates/core/src/lib.rs", text)
            .into_iter()
            .map(|finding| finding.line)
            .collect();
        assert_eq!(lines, vec![Some(1), Some(3)]);
    }
}
