use proc_macro2::{Delimiter, TokenStream, TokenTree};

use crate::finding::Finding;

pub const CRATE_ROOT_ATTRIBUTE: &str = "cfg_attr(not(any(test, feature = \"ts\")), no_std)";
const STD_FEATURE: &str = "ts";
const FIX: &str = "move the file, process, clock or environment access into wb-core";

pub struct SourceFile<'a> {
    pub path: &'a str,
    pub text: &'a str,
}

struct Scan<'a> {
    path: &'a str,
    is_root: bool,
    root_attributes: Vec<String>,
    root_names_alloc: bool,
    findings: Vec<Finding>,
}

pub fn findings(crate_root: &str, files: &[SourceFile]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for file in files {
        let stream = match file.text.parse::<TokenStream>() {
            Ok(stream) => stream,
            Err(error) => {
                findings.push(Finding::whole(
                    file.path,
                    format!("cannot be read as Rust tokens: {error}"),
                ));
                continue;
            }
        };
        let mut scan = Scan {
            path: file.path,
            is_root: file.path == crate_root,
            root_attributes: Vec::new(),
            root_names_alloc: false,
            findings: Vec::new(),
        };
        scan.level(stream, true);
        if scan.is_root {
            let expected = normalized(CRATE_ROOT_ATTRIBUTE);
            if scan.root_attributes != [expected] || !scan.root_names_alloc {
                scan.findings.push(Finding::whole(
                    file.path,
                    format!(
                        "the crate root must carry exactly `#![{CRATE_ROOT_ATTRIBUTE}]` and `extern crate alloc;`, so that the compiler refuses file, process, network and clock access"
                    ),
                ));
            }
        }
        findings.extend(scan.findings);
    }
    findings
}

fn normalized(text: &str) -> String {
    text.parse::<TokenStream>()
        .map_or_else(|_| text.to_owned(), |stream| stream.to_string())
}

fn is_ident(token: Option<&TokenTree>, name: &str) -> bool {
    matches!(token, Some(TokenTree::Ident(ident)) if ident.to_string().trim_start_matches("r#") == name)
}

fn is_punct(token: Option<&TokenTree>, character: char) -> bool {
    matches!(token, Some(TokenTree::Punct(punct)) if punct.as_char() == character)
}

fn line(token: &TokenTree) -> usize {
    token.span().start().line
}

fn split_commas(stream: TokenStream) -> Vec<Vec<TokenTree>> {
    let mut parts = vec![Vec::new()];
    for token in stream {
        if is_punct(Some(&token), ',') {
            parts.push(Vec::new());
        } else if let Some(last) = parts.last_mut() {
            last.push(token);
        }
    }
    parts.retain(|part| !part.is_empty());
    parts
}

fn names_std_feature(stream: TokenStream) -> bool {
    let tokens: Vec<TokenTree> = stream.into_iter().collect();
    tokens.iter().enumerate().any(|(index, token)| match token {
        TokenTree::Group(group) => names_std_feature(group.stream()),
        TokenTree::Ident(_) => {
            is_ident(Some(token), "feature")
                && is_punct(tokens.get(index + 1), '=')
                && matches!(tokens.get(index + 2), Some(TokenTree::Literal(literal)) if literal.to_string() == format!("\"{STD_FEATURE}\""))
        }
        _ => false,
    })
}

fn parenthesized(tokens: &[TokenTree], name: &str) -> Option<TokenStream> {
    match tokens {
        [first, TokenTree::Group(group)]
            if is_ident(Some(first), name) && group.delimiter() == Delimiter::Parenthesis =>
        {
            Some(group.stream())
        }
        _ => None,
    }
}

fn gates_only_the_export_derive(attribute: &TokenStream) -> bool {
    let tokens: Vec<TokenTree> = attribute.clone().into_iter().collect();
    let Some(arguments) = parenthesized(&tokens, "cfg_attr") else {
        return false;
    };
    let parts = split_commas(arguments);
    let Some((predicate, attributes)) = parts.split_first() else {
        return false;
    };
    let predicate: TokenStream = predicate.iter().cloned().collect();
    predicate.to_string() == normalized(&format!("feature = \"{STD_FEATURE}\""))
        && !attributes.is_empty()
        && attributes.iter().all(|attribute| {
            parenthesized(attribute, "ts").is_some()
                || parenthesized(attribute, "derive").is_some_and(|derives| {
                    split_commas(derives)
                        .iter()
                        .all(|path| is_ident(path.last(), "TS"))
                })
        })
}

impl Scan<'_> {
    fn report(&mut self, token: &TokenTree, message: &str) {
        self.findings.push(Finding::new(
            self.path,
            line(token),
            format!("{message}; {FIX}"),
        ));
    }

    fn level(&mut self, stream: TokenStream, top: bool) {
        let tokens: Vec<TokenTree> = stream.into_iter().collect();
        for (index, token) in tokens.iter().enumerate() {
            match token {
                TokenTree::Group(group) => self.level(group.stream(), false),
                TokenTree::Ident(_)
                    if is_ident(Some(token), "extern")
                        && is_ident(tokens.get(index + 1), "crate") =>
                {
                    let name = tokens
                        .get(index + 2)
                        .map(ToString::to_string)
                        .unwrap_or_default();
                    if self.is_root && top && name == "alloc" && !self.root_names_alloc {
                        self.root_names_alloc = true;
                    } else {
                        self.report(
                            token,
                            &format!(
                                "names `extern crate {name}`; only the crate root of wb-model names a crate, and only `alloc`"
                            ),
                        );
                    }
                }
                TokenTree::Ident(_)
                    if is_ident(Some(token), "include") && is_punct(tokens.get(index + 1), '!') =>
                {
                    self.report(
                        token,
                        "includes source text with `include!`; keep wb-model's code in its own module files",
                    );
                }
                TokenTree::Punct(_) if is_punct(Some(token), '#') => {
                    let inner = is_punct(tokens.get(index + 1), '!');
                    let attribute = match tokens.get(index + if inner { 2 } else { 1 }) {
                        Some(TokenTree::Group(group))
                            if group.delimiter() == Delimiter::Bracket =>
                        {
                            group.stream()
                        }
                        _ => continue,
                    };
                    self.attribute(token, &attribute, inner && top);
                }
                _ => {}
            }
        }
    }

    fn attribute(&mut self, at: &TokenTree, attribute: &TokenStream, top_inner: bool) {
        let first: Vec<TokenTree> = attribute.clone().into_iter().take(1).collect();
        if is_ident(first.first(), "path") {
            self.report(
                at,
                "loads a module from a chosen file with `#[path]`; keep wb-model's code in its own module files",
            );
        }
        let text = attribute.to_string();
        if self.is_root && top_inner {
            self.root_attributes.push(text.clone());
        }
        let is_crate_root_attribute =
            self.is_root && top_inner && text == normalized(CRATE_ROOT_ATTRIBUTE);
        if names_std_feature(attribute.clone())
            && !is_crate_root_attribute
            && !gates_only_the_export_derive(attribute)
        {
            self.report(
                at,
                &format!(
                    "gates `#[{text}]` on the `{STD_FEATURE}` feature, which compiles wb-model with std; gate only `derive(ts_rs::TS)` and `ts(...)` on it"
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SourceFile, findings};

    const ROOT: &str = "crates/model/src/lib.rs";
    const GOOD_ROOT: &str = "#![cfg_attr(not(any(test, feature = \"ts\")), no_std)]\n\nextern crate alloc;\n\nmod id;\n";

    fn messages(files: &[(&str, &str)]) -> Vec<String> {
        let files: Vec<SourceFile> = files
            .iter()
            .map(|(path, text)| SourceFile { path, text })
            .collect();
        findings(ROOT, &files)
            .into_iter()
            .map(|finding| finding.to_string())
            .collect()
    }

    #[test]
    fn the_intended_crate_root_and_export_derives_pass() {
        let id = "#[derive(Serialize)]\n#[cfg_attr(feature = \"ts\", derive(ts_rs::TS), ts(type = \"string\"))]\npub struct NoteId(String);\n#[cfg(test)]\nmod tests { #[test] fn reads() { assert!(std::fs::metadata(\".\").is_ok()); } }\n";
        assert_eq!(
            messages(&[(ROOT, GOOD_ROOT), ("crates/model/src/id.rs", id)]),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_crate_root_without_the_no_std_attribute_fails() {
        for root in [
            "extern crate alloc;\n",
            "#![no_std]\nextern crate alloc;\n",
            "#![cfg_attr(not(any(test, feature = \"ts\")), no_std)]\n",
            "#![cfg_attr(not(any(test, feature = \"ts\")), no_std)]\n#![allow(unused)]\nextern crate alloc;\n",
        ] {
            let found = messages(&[(ROOT, root)]);
            assert_eq!(found.len(), 1, "{root}");
            assert!(
                found[0].starts_with("crates/model/src/lib.rs: the crate root must carry exactly")
            );
        }
    }

    #[test]
    fn std_reached_around_the_crate_root_is_reported() {
        let sneaky = "mod a { extern crate std; }\nmacro_rules! m { () => { extern crate std as s; } }\ninclude!(\"../impure.inc\");\n#[path = \"../impure.rs\"]\nmod impure;\n";
        let found = messages(&[(ROOT, GOOD_ROOT), ("crates/model/src/a.rs", sneaky)]);
        let lines: Vec<&str> = found
            .iter()
            .map(|message| message.split(": ").next().unwrap_or_default())
            .collect();
        assert_eq!(
            lines,
            vec![
                "crates/model/src/a.rs:1",
                "crates/model/src/a.rs:2",
                "crates/model/src/a.rs:3",
                "crates/model/src/a.rs:4",
            ]
        );
        assert!(found[0].contains("names `extern crate std`"));
        assert!(found[2].contains("include!"));
        assert!(found[3].contains("#[path]"));
    }

    #[test]
    fn code_behind_the_ts_feature_is_reported() {
        let gated = "#[cfg(feature = \"ts\")]\npub fn read() {}\n#[cfg_attr(feature = \"ts\", derive(ts_rs::TS, Debug))]\npub struct A;\n#[cfg_attr(feature = \"ts\", path = \"x.rs\")]\nmod b;\n";
        let found = messages(&[(ROOT, GOOD_ROOT), ("crates/model/src/a.rs", gated)]);
        assert_eq!(found.len(), 3);
        assert!(found[0].starts_with("crates/model/src/a.rs:1: gates `#[cfg"));
        assert!(found[0].contains("on the `ts` feature, which compiles wb-model with std"));
    }
}
