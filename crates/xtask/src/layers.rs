use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

use serde_json::Value;

use crate::finding::{self, Finding};
use crate::process;
use crate::purity::{self, SourceFile};

struct Layer {
    package: &'static str,
    workspace: &'static [&'static str],
    forbidden: &'static [&'static str],
    fix: &'static str,
    pure: bool,
}

const LAYERS: &[Layer] = &[
    Layer {
        package: "wb-model",
        workspace: &[],
        forbidden: &["tauri", "clap", "notify", "tokio"],
        fix: "keep wb-model pure: move the dependency into wb-core or a binary",
        pure: true,
    },
    Layer {
        package: "wb-core",
        workspace: &["wb-model"],
        forbidden: &["tauri", "clap"],
        fix: "move command-line and window code into wb or whiteboard",
        pure: false,
    },
    Layer {
        package: "wb",
        workspace: &["wb-core", "wb-model"],
        forbidden: &["tauri"],
        fix: "move window code into whiteboard",
        pure: false,
    },
    Layer {
        package: "whiteboard",
        workspace: &["wb-core", "wb-model"],
        forbidden: &[],
        fix: "move shared code into wb-core or wb-model",
        pure: false,
    },
    Layer {
        package: "xtask",
        workspace: &["wb-model"],
        forbidden: &[],
        fix: "xtask reads wb-model for the bindings and nothing else",
        pure: false,
    },
];

#[derive(Default)]
struct Graph {
    workspace: BTreeSet<String>,
    edges: BTreeMap<String, Vec<Edge>>,
    packages: BTreeMap<String, Package>,
}

#[derive(Default)]
struct Package {
    manifest: String,
    library_root: Option<String>,
    build_script: Option<String>,
}

struct Edge {
    to: String,
    dev_only: bool,
}

pub fn check(root: &Path) -> Result<(), String> {
    let metadata = process::output(
        root,
        &["cargo", "metadata", "--locked", "--format-version", "1"],
    )?;
    let graph = parse_metadata(&metadata, root)?;
    let mut findings = graph_findings(&graph);
    let tracked = finding::tracked_files(root)?;
    for layer in LAYERS.iter().filter(|layer| layer.pure) {
        if let Some(package) = graph.packages.get(layer.package) {
            findings.extend(purity_findings(root, package, &tracked));
        }
    }
    finding::report(&findings)
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
    let folder = package.manifest.trim_end_matches("Cargo.toml");
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
    Package {
        manifest: relative(root, package["manifest_path"].as_str()).unwrap_or_default(),
        library_root: source_of("lib"),
        build_script: source_of("custom-build"),
    }
}

fn parse_metadata(json: &str, root: &Path) -> Result<Graph, String> {
    let metadata: Value =
        serde_json::from_str(json).map_err(|error| format!("cargo metadata: {error}"))?;
    let names: BTreeMap<&str, &str> = metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|package| Some((package["id"].as_str()?, package["name"].as_str()?)))
        .collect();
    let name_of = |id: &Value| {
        id.as_str()
            .and_then(|id| names.get(id))
            .map(|name| (*name).to_owned())
    };
    let mut graph = Graph {
        workspace: metadata["workspace_members"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(name_of)
            .collect(),
        ..Graph::default()
    };
    for package in metadata["packages"].as_array().into_iter().flatten() {
        if let Some(name) = name_of(&package["id"]).filter(|name| graph.workspace.contains(name)) {
            graph.packages.insert(name, package_facts(package, root));
        }
    }
    for node in metadata["resolve"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let Some(from) = name_of(&node["id"]) else {
            continue;
        };
        let edges = node["deps"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|dependency| {
                let to = name_of(&dependency["pkg"])?;
                let dev_only = dependency["dep_kinds"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .all(|kind| kind["kind"].as_str() == Some("dev"));
                Some(Edge { to, dev_only })
            })
            .collect();
        graph.edges.insert(from, edges);
    }
    Ok(graph)
}

fn path_to(graph: &Graph, from: &str, target: &str) -> Option<Vec<String>> {
    let mut came_from: BTreeMap<&str, &str> = BTreeMap::new();
    let mut queue = VecDeque::from([from]);
    while let Some(current) = queue.pop_front() {
        if current == target {
            let mut path = vec![current.to_owned()];
            let mut step = current;
            while let Some(previous) = came_from.get(step) {
                path.push((*previous).to_owned());
                step = previous;
            }
            path.reverse();
            return Some(path);
        }
        for edge in graph.edges.get(current).into_iter().flatten() {
            if !edge.dev_only && edge.to != from && !came_from.contains_key(edge.to.as_str()) {
                came_from.insert(&edge.to, current);
                queue.push_back(&edge.to);
            }
        }
    }
    None
}

fn graph_findings(graph: &Graph) -> Vec<Finding> {
    let mut findings = Vec::new();
    let manifest = |package: &str| format!("package {package}");
    for package in &graph.workspace {
        let Some(layer) = LAYERS.iter().find(|layer| layer.package == package) else {
            findings.push(Finding::new(
                &manifest(package),
                1,
                "is missing from the layer table; add it to LAYERS in crates/xtask/src/layers.rs with its allowed dependencies",
            ));
            continue;
        };
        for edge in graph.edges.get(package).into_iter().flatten() {
            if graph.workspace.contains(&edge.to) && !layer.workspace.contains(&edge.to.as_str()) {
                findings.push(Finding::new(
                    &manifest(package),
                    1,
                    format!(
                        "depends on {}, which the layer table forbids; {}",
                        edge.to, layer.fix
                    ),
                ));
            }
        }
        for forbidden in layer.forbidden {
            if let Some(path) = path_to(graph, package, forbidden) {
                findings.push(Finding::new(
                    &manifest(package),
                    1,
                    format!(
                        "depends on {forbidden} through {}, which the layer table forbids; {}",
                        path.join(" -> "),
                        layer.fix
                    ),
                ));
            }
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::{Edge, Graph, graph_findings};

    fn graph(workspace: &[&str], edges: &[(&str, &str, bool)]) -> Graph {
        let mut graph = Graph {
            workspace: workspace.iter().map(|name| (*name).to_owned()).collect(),
            ..Graph::default()
        };
        for (from, to, dev_only) in edges {
            graph
                .edges
                .entry((*from).to_owned())
                .or_default()
                .push(Edge {
                    to: (*to).to_owned(),
                    dev_only: *dev_only,
                });
        }
        graph
    }

    fn messages(graph: &Graph) -> Vec<String> {
        graph_findings(graph)
            .into_iter()
            .map(|finding| finding.to_string())
            .collect()
    }

    const MEMBERS: [&str; 5] = ["wb-model", "wb-core", "wb", "whiteboard", "xtask"];

    #[test]
    fn the_intended_graph_passes() {
        let intended = graph(
            &MEMBERS,
            &[
                ("wb-core", "wb-model", false),
                ("wb", "wb-core", false),
                ("wb", "wb-model", false),
                ("wb", "clap", false),
                ("whiteboard", "wb-core", false),
                ("whiteboard", "tauri", false),
                ("xtask", "wb-model", false),
                ("wb-model", "serde", false),
            ],
        );
        assert_eq!(messages(&intended), Vec::<String>::new());
    }

    #[test]
    fn a_forbidden_workspace_edge_is_named_with_its_fix() {
        let inverted = graph(
            &MEMBERS,
            &[
                ("wb-core", "wb-model", false),
                ("wb-model", "wb-core", true),
            ],
        );
        assert_eq!(
            messages(&inverted),
            vec![
                "package wb-model:1: depends on wb-core, which the layer table forbids; keep wb-model pure: move the dependency into wb-core or a binary"
            ]
        );
    }

    #[test]
    fn a_forbidden_crate_is_found_through_other_crates() {
        let indirect = graph(
            &MEMBERS,
            &[("wb-core", "watcher", false), ("watcher", "clap", false)],
        );
        assert_eq!(
            messages(&indirect),
            vec![
                "package wb-core:1: depends on clap through wb-core -> watcher -> clap, which the layer table forbids; move command-line and window code into wb or whiteboard"
            ]
        );
    }

    #[test]
    fn a_forbidden_crate_used_only_by_tests_is_allowed() {
        let dev = graph(&MEMBERS, &[("wb-model", "tokio", true)]);
        assert_eq!(messages(&dev), Vec::<String>::new());
    }

    #[test]
    fn a_package_missing_from_the_table_fails() {
        let extra = graph(&["wb-model", "wb-sync"], &[]);
        assert_eq!(messages(&extra).len(), 1);
        assert!(
            messages(&extra)[0].starts_with("package wb-sync:1: is missing from the layer table")
        );
    }
}
