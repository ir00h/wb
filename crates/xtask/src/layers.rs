use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use crate::finding::{self, Finding};
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
        normal: &["serde", "thiserror", "ts-rs"],
        build: &[],
        dev: &["serde_json"],
    },
    Layer {
        package: "wb-core",
        role: "boards on disk",
        pure: false,
        normal: &["wb-model"],
        build: &[],
        dev: &[],
    },
    Layer {
        package: "wb",
        role: "the command-line tool",
        pure: false,
        normal: &["clap", "wb-core", "wb-model"],
        build: &[],
        dev: &[],
    },
    Layer {
        package: "whiteboard",
        role: "the desktop shell",
        pure: false,
        normal: &["tauri", "wb-core", "wb-model"],
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

#[derive(Default)]
struct Package {
    manifest: String,
    library_root: Option<String>,
    build_script: Option<String>,
    dependencies: Vec<(String, Kind)>,
}

pub fn check(root: &Path) -> Result<(), String> {
    let metadata = process::output(
        root,
        &["cargo", "metadata", "--locked", "--format-version", "1"],
    )?;
    let packages = parse_metadata(&metadata, root)?;
    let tracked = finding::tracked_files(root)?;
    let mut findings = table_findings(&packages);
    findings.extend(nested_workspaces(root, &tracked));
    for layer in LAYERS.iter().filter(|layer| layer.pure) {
        if let Some(package) = packages.get(layer.package) {
            findings.extend(purity_findings(root, package, &tracked));
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

fn purity_findings(root: &Path, package: &Package, tracked: &[String]) -> Vec<Finding> {
    let mut findings = Vec::new();
    if let Some(build_script) = &package.build_script {
        findings.push(Finding::whole(
            build_script,
            "a pure package has no build script, because a build script runs with file, process and network access; move the work into wb-core or xtask",
        ));
    }
    let Some(crate_root) = &package.library_root else {
        findings.push(Finding::whole(
            &package.manifest,
            "a pure package must be a library",
        ));
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
    let source_of = |kind: &str| {
        targets
            .iter()
            .find(|target| {
                target["kind"]
                    .as_array()
                    .is_some_and(|kinds| kinds.iter().any(|named| named.as_str() == Some(kind)))
            })
            .and_then(|target| relative(root, target["src_path"].as_str()))
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
        library_root: source_of("lib"),
        build_script: source_of("custom-build"),
        dependencies,
    }
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

    use super::{Kind, Package, declares_workspace, table_findings};

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
                &[("serde_json", Kind::Normal), ("wb-core", Kind::Dev)],
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
                "crates/wb-model/Cargo.toml: wb-model (pure shapes and rules) depends directly on serde_json as a normal dependency; add it to the table in `layers.rs` if the layer may use it",
                "crates/wb-model/Cargo.toml: wb-model (pure shapes and rules) depends directly on wb-core as a dev dependency; add it to the table in `layers.rs` if the layer may use it",
            ]
        );
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
    fn a_manifest_with_its_own_workspace_is_found() {
        assert!(declares_workspace(
            "[package]\nname = \"helper\"\n\n[workspace]\n"
        ));
        assert!(declares_workspace("workspace = { members = [] }\n"));
        assert!(!declares_workspace(
            "[package]\nname = \"wb\"\nversion.workspace = true\n\n[dependencies]\nclap.workspace = true\n"
        ));
    }
}
