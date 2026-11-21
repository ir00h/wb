use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

use ra_ap_rustc_lexer::{FrontmatterAllowed, TokenKind, tokenize};
use serde_json::Value;

use crate::finding::{self, Finding, line_of};
use crate::process;

struct Layer {
    package: &'static str,
    workspace: &'static [&'static str],
    forbidden: &'static [&'static str],
    fix: &'static str,
}

const LAYERS: &[Layer] = &[
    Layer {
        package: "wb-model",
        workspace: &[],
        forbidden: &["tauri", "clap", "notify", "tokio"],
        fix: "keep wb-model pure: move the dependency into wb-core or a binary",
    },
    Layer {
        package: "wb-core",
        workspace: &["wb-model"],
        forbidden: &["tauri", "clap"],
        fix: "move command-line and window code into wb or whiteboard",
    },
    Layer {
        package: "wb",
        workspace: &["wb-core", "wb-model"],
        forbidden: &["tauri"],
        fix: "move window code into whiteboard",
    },
    Layer {
        package: "whiteboard",
        workspace: &["wb-core", "wb-model"],
        forbidden: &[],
        fix: "move shared code into wb-core or wb-model",
    },
    Layer {
        package: "xtask",
        workspace: &["wb-model"],
        forbidden: &[],
        fix: "xtask reads wb-model for the bindings and nothing else",
    },
];

const PURE_PACKAGE_SOURCE: &str = "crates/model/src/";
const IMPURE_STD_MODULES: [&str; 5] = ["fs", "process", "net", "env", "thread"];
const IMPURE_NAMES: [&str; 1] = ["SystemTime"];

#[derive(Default)]
struct Graph {
    workspace: BTreeSet<String>,
    edges: BTreeMap<String, Vec<Edge>>,
}

struct Edge {
    to: String,
    dev_only: bool,
}

pub fn check(root: &Path) -> Result<(), String> {
    let metadata = process::output(root, &["cargo", "metadata", "--format-version", "1"])?;
    let graph = parse_metadata(&metadata)?;
    let mut findings = graph_findings(&graph);
    for path in finding::tracked_files(root)? {
        if path.starts_with(PURE_PACKAGE_SOURCE)
            && Path::new(&path)
                .extension()
                .is_some_and(|extension| extension == "rs")
            && let Ok(text) = std::fs::read_to_string(root.join(&path))
        {
            findings.extend(impure_uses(&path, &text));
        }
    }
    finding::report(&findings)
}

fn parse_metadata(json: &str) -> Result<Graph, String> {
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

fn impure_uses(path: &str, text: &str) -> Vec<Finding> {
    let mut tokens: Vec<(TokenKind, usize, &str)> = Vec::new();
    let mut offset = 0;
    for token in tokenize(text, FrontmatterAllowed::No) {
        let length = token.len as usize;
        if !matches!(
            token.kind,
            TokenKind::Whitespace | TokenKind::LineComment { .. } | TokenKind::BlockComment { .. }
        ) {
            tokens.push((token.kind, offset, &text[offset..offset + length]));
        }
        offset += length;
    }
    let is_path_separator = |index: usize| {
        matches!(tokens.get(index), Some((TokenKind::Colon, _, _)))
            && matches!(tokens.get(index + 1), Some((TokenKind::Colon, _, _)))
    };
    let mut found: Vec<(usize, String)> = Vec::new();
    for (index, &(kind, at, word)) in tokens.iter().enumerate() {
        if kind != TokenKind::Ident {
            continue;
        }
        if IMPURE_NAMES.contains(&word) {
            found.push((at, word.to_owned()));
        }
        if word != "std" || !is_path_separator(index + 1) {
            continue;
        }
        let mut depth = 0usize;
        let mut segment_start = true;
        for &(next_kind, next_at, next_word) in &tokens[index + 3..] {
            match next_kind {
                TokenKind::OpenBrace => {
                    depth += 1;
                    segment_start = true;
                }
                TokenKind::CloseBrace => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        break;
                    }
                }
                TokenKind::Comma if depth == 1 => segment_start = true,
                TokenKind::Ident if segment_start => {
                    if IMPURE_STD_MODULES.contains(&next_word) {
                        found.push((next_at, format!("std::{next_word}")));
                    }
                    segment_start = false;
                    if depth == 0 {
                        break;
                    }
                }
                _ if depth == 0 => break,
                _ => segment_start = false,
            }
        }
    }
    found.sort();
    found
        .into_iter()
        .map(|(at, name)| {
            Finding::new(
                path,
                line_of(text, at),
                format!(
                    "wb-model names {name}, but it holds only pure shapes and rules; move the file, process, clock or environment access into wb-core"
                ),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Edge, Graph, graph_findings, impure_uses};

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

    fn impure_lines(text: &str) -> Vec<(usize, String)> {
        impure_uses("crates/model/src/lib.rs", text)
            .into_iter()
            .map(|finding| {
                let name = finding
                    .message
                    .trim_start_matches("wb-model names ")
                    .split(',')
                    .next()
                    .unwrap_or_default()
                    .to_owned();
                (finding.line, name)
            })
            .collect()
    }

    #[test]
    fn side_effects_in_the_model_are_reported() {
        let text = "use std::fs;\nuse std::{collections::BTreeMap, env, io};\nfn now() { let _ = std::time::SystemTime::now(); }\nfn spawn() { ::std::thread::spawn(|| {}); }\nuse std::process::Command;\nuse std::net::TcpStream;\n";
        assert_eq!(
            impure_lines(text),
            vec![
                (1, "std::fs".to_owned()),
                (2, "std::env".to_owned()),
                (3, "SystemTime".to_owned()),
                (4, "std::thread".to_owned()),
                (5, "std::process".to_owned()),
                (6, "std::net".to_owned()),
            ]
        );
    }

    #[test]
    fn pure_code_and_strings_that_name_side_effects_pass() {
        let text = "use std::fmt;\nuse std::collections::BTreeMap;\nconst HELP: &str = \"std::fs is not used here\";\nfn fs() {}\n";
        assert_eq!(impure_lines(text), Vec::<(usize, String)>::new());
    }
}
