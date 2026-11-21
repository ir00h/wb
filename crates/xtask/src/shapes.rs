use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::{TokenStream, TokenTree};
use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};
use syn::{Attribute, Expr, Ident, Lit, Meta, MetaList, Path, Token, Type};

use crate::finding::Finding;

pub const NEVER_CROSSES: &[(&str, &str)] = &[
    (
        "BoardConfig",
        "the board config file stays on disk; the window sees its sections as SectionNode",
    ),
    (
        "TagColour",
        "a tag entry of the board config file, which stays on disk",
    ),
    (
        "SectionEntry",
        "a section entry of the board config file; the window sees sections as SectionNode",
    ),
    (
        "BoardFile",
        "the board file stays on disk and is never sent to the window",
    ),
];
const EXPORT_LIST: &str = "export_every_type in crates/xtask/src/bindings.rs";

#[derive(Default)]
struct Shape {
    path: String,
    line: usize,
    exported_as: Option<String>,
    serialized: bool,
}

#[derive(Default)]
struct Traits {
    exports: bool,
    serialized: bool,
    ts_name: Option<String>,
    serde_name: Option<String>,
}

impl Traits {
    fn exported_name(self, ident: &Ident) -> String {
        self.ts_name
            .or(self.serde_name)
            .unwrap_or_else(|| ident.to_string())
    }
}

pub struct SourceFile<'a> {
    pub path: &'a str,
    pub text: &'a str,
}

struct Collector<'a> {
    path: &'a str,
    shapes: BTreeMap<String, Shape>,
    findings: Vec<Finding>,
}

pub fn findings(
    files: &[SourceFile],
    generated: &BTreeSet<String>,
    never_crosses: &[(&str, &str)],
) -> Vec<Finding> {
    let mut shapes: BTreeMap<String, Shape> = BTreeMap::new();
    let mut findings = Vec::new();
    for file in files {
        let parsed = match syn::parse_file(file.text) {
            Ok(parsed) => parsed,
            Err(error) => {
                findings.push(Finding::new(
                    file.path,
                    error.span().start().line,
                    format!("cannot be parsed as Rust, so its shapes cannot be checked: {error}"),
                ));
                continue;
            }
        };
        let mut collector = Collector {
            path: file.path,
            shapes: std::mem::take(&mut shapes),
            findings: Vec::new(),
        };
        collector.visit_file(&parsed);
        shapes = collector.shapes;
        findings.extend(collector.findings);
    }
    for (name, shape) in &shapes {
        match &shape.exported_as {
            Some(exported) if !generated.contains(exported) => findings.push(Finding::new(
                &shape.path,
                shape.line,
                format!(
                    "`{name}` derives the TypeScript export but no binding is generated for it; add `{name}::export_all` to {EXPORT_LIST}"
                ),
            )),
            None if shape.serialized
                && !never_crosses.iter().any(|(never, _)| never == name) =>
            {
                findings.push(Finding::new(
                    &shape.path,
                    shape.line,
                    format!(
                        "`{name}` is serialized but derives no TypeScript export, so the window would write its shape by hand; add `#[cfg_attr(feature = \"ts\", derive(ts_rs::TS))]`, or add it to NEVER_CROSSES in crates/xtask/src/shapes.rs with the reason it never reaches the window"
                    ),
                ));
            }
            _ => {}
        }
    }
    findings
}

pub fn declared_names(text: &str) -> impl Iterator<Item = String> + '_ {
    text.lines().filter_map(|line| {
        let declaration = line.strip_prefix("export type ")?;
        let end = declaration
            .find(|c: char| c == '<' || c == '=' || c.is_whitespace())
            .unwrap_or(declaration.len());
        Some(declaration[..end].to_owned())
    })
}

fn is_test_only(attributes: &[Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        attribute.path().is_ident("cfg")
            && attribute
                .parse_args::<Ident>()
                .is_ok_and(|predicate| predicate == "test")
    })
}

fn last_name(path: &Path) -> String {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
        .unwrap_or_default()
}

fn read_meta(meta: &Meta, traits: &mut Traits) {
    let Meta::List(list) = meta else {
        return;
    };
    if list.path.is_ident("derive") {
        let derived = list
            .parse_args_with(Punctuated::<Path, Token![,]>::parse_terminated)
            .unwrap_or_default();
        for path in &derived {
            match last_name(path).as_str() {
                "TS" => traits.exports = true,
                "Serialize" | "Deserialize" => traits.serialized = true,
                _ => {}
            }
        }
    } else if list.path.is_ident("cfg_attr") {
        let nested = list
            .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
            .unwrap_or_default();
        for inner in nested.iter().skip(1) {
            read_meta(inner, traits);
        }
    } else if list.path.is_ident("ts") {
        if let Some(name) = renamed(list) {
            traits.ts_name = Some(name);
        }
    } else if list.path.is_ident("serde")
        && let Some(name) = renamed(list)
    {
        traits.serde_name = Some(name);
    }
}

fn renamed(list: &MetaList) -> Option<String> {
    let nested = list
        .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        .unwrap_or_default();
    nested.iter().rev().find_map(|inner| match inner {
        Meta::NameValue(pair) if pair.path.is_ident("rename") => match &pair.value {
            Expr::Lit(literal) => match &literal.lit {
                Lit::Str(text) => Some(text.value()),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    })
}

fn names_derive(stream: TokenStream) -> bool {
    stream.into_iter().any(|token| match token {
        TokenTree::Ident(ident) => ident == "derive",
        TokenTree::Group(group) => names_derive(group.stream()),
        _ => false,
    })
}

impl Collector<'_> {
    fn shape(&mut self, ident: &Ident) -> &mut Shape {
        let path = self.path;
        self.shapes
            .entry(ident.to_string())
            .or_insert_with(|| Shape {
                path: path.to_owned(),
                line: ident.span().start().line,
                ..Shape::default()
            })
    }

    fn declared(&mut self, ident: &Ident, attributes: &[Attribute]) {
        let mut traits = Traits::default();
        for attribute in attributes {
            read_meta(&attribute.meta, &mut traits);
        }
        let path = self.path.to_owned();
        let shape = self.shape(ident);
        shape.path = path;
        shape.line = ident.span().start().line;
        shape.serialized |= traits.serialized;
        if traits.exports {
            shape.exported_as = Some(traits.exported_name(ident));
        }
    }
}

impl<'ast> Visit<'ast> for Collector<'_> {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if !is_test_only(&item.attrs) {
            visit::visit_item_mod(self, item);
        }
    }

    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if !is_test_only(&item.attrs) {
            visit::visit_item_fn(self, item);
        }
    }

    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        if !is_test_only(&item.attrs) {
            self.declared(&item.ident, &item.attrs);
        }
    }

    fn visit_item_enum(&mut self, item: &'ast syn::ItemEnum) {
        if !is_test_only(&item.attrs) {
            self.declared(&item.ident, &item.attrs);
        }
    }

    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        let Some((_, trait_path, _)) = &item.trait_ else {
            return;
        };
        let Type::Path(self_type) = item.self_ty.as_ref() else {
            return;
        };
        let Some(segment) = self_type.path.segments.last() else {
            return;
        };
        let ident = segment.ident.clone();
        match last_name(trait_path).as_str() {
            "Serialize" | "Deserialize" => self.shape(&ident).serialized = true,
            "TS" => {
                let shape = self.shape(&ident);
                shape.exported_as.get_or_insert_with(|| ident.to_string());
            }
            _ => {}
        }
    }

    fn visit_item_macro(&mut self, item: &'ast syn::ItemMacro) {
        if item.mac.path.is_ident("macro_rules") && names_derive(item.mac.tokens.clone()) {
            let line = item
                .ident
                .as_ref()
                .map_or(1, |ident| ident.span().start().line);
            self.findings.push(Finding::new(
                self.path,
                line,
                "a macro that writes `derive` hides its shapes from the bindings check; declare each type with its derives in plain source",
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{SourceFile, declared_names, findings};

    const PATH: &str = "crates/model/src/card.rs";

    fn messages(text: &str, generated: &[&str], never_crosses: &[(&str, &str)]) -> Vec<String> {
        let generated: BTreeSet<String> = generated.iter().map(|name| (*name).to_owned()).collect();
        findings(
            &[SourceFile { path: PATH, text }],
            &generated,
            never_crosses,
        )
        .into_iter()
        .map(|finding| finding.to_string())
        .collect()
    }

    #[test]
    fn an_exported_shape_with_a_binding_passes() {
        let text = "#[derive(Serialize)]\n#[cfg_attr(feature = \"ts\", derive(ts_rs::TS), ts(rename = \"Card\"))]\npub struct CardShape { x: u32 }\n";
        assert_eq!(messages(text, &["Card"], &[]), Vec::<String>::new());
    }

    #[test]
    fn an_exported_shape_is_looked_up_by_its_serde_name_unless_ts_renames_it() {
        let serde_only = "#[derive(Debug, Clone, serde::Serialize)]\n#[cfg_attr(feature = \"ts\", derive(ts_rs::TS))]\n#[serde(rename = \"Placement\")]\npub struct CardPlacement { pub x: u32 }\n";
        assert_eq!(
            messages(serde_only, &["Placement"], &[]),
            Vec::<String>::new()
        );
        assert_eq!(
            messages(serde_only, &["CardPlacement"], &[]),
            vec![
                "crates/model/src/card.rs:4: `CardPlacement` derives the TypeScript export but no binding is generated for it; add `CardPlacement::export_all` to export_every_type in crates/xtask/src/bindings.rs"
            ]
        );
        let both = "#[cfg_attr(feature = \"ts\", derive(ts_rs::TS), ts(rename = \"Spot\"))]\n#[derive(serde::Serialize)]\n#[serde(rename = \"Placement\", deny_unknown_fields)]\npub struct CardPlacement { pub x: u32 }\n";
        assert_eq!(messages(both, &["Spot"], &[]), Vec::<String>::new());
    }

    #[test]
    fn an_exported_shape_without_a_binding_is_reported() {
        let text = "#[derive(Debug, Clone, serde::Serialize)]\n#[cfg_attr(feature = \"ts\", derive(ts_rs::TS))]\npub struct CardPosition { pub x: u32, pub y: u32 }\n";
        assert_eq!(
            messages(text, &["NoteId"], &[]),
            vec![
                "crates/model/src/card.rs:3: `CardPosition` derives the TypeScript export but no binding is generated for it; add `CardPosition::export_all` to export_every_type in crates/xtask/src/bindings.rs"
            ]
        );
    }

    #[test]
    fn a_serialized_shape_without_the_export_is_reported_unless_it_never_crosses() {
        let text = "mod inner {\n    #[derive(Serialize)]\n    pub enum Layout { Grid }\n}\npub struct Plain;\nimpl serde::Deserialize<'_> for Plain {}\n";
        let found = messages(text, &[], &[]);
        assert_eq!(found.len(), 2);
        assert!(found[0].starts_with(
            "crates/model/src/card.rs:3: `Layout` is serialized but derives no TypeScript export"
        ));
        assert!(found[1].starts_with("crates/model/src/card.rs:5: `Plain` is serialized"));
        assert_eq!(
            messages(
                text,
                &[],
                &[
                    ("Layout", "stored on disk only"),
                    ("Plain", "stored on disk only")
                ]
            ),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_macro_that_writes_derives_is_reported() {
        let text = "macro_rules! shape {\n    ($name:ident) => {\n        #[derive(Serialize)]\n        pub struct $name;\n    };\n}\n";
        let found = messages(text, &[], &[]);
        assert_eq!(found.len(), 1);
        assert!(found[0].starts_with("crates/model/src/card.rs:1: a macro that writes `derive`"));
    }

    #[test]
    fn test_only_shapes_are_ignored() {
        let text =
            "#[cfg(test)]\nmod tests {\n    #[derive(serde::Serialize)]\n    struct Probe;\n}\n";
        assert_eq!(messages(text, &[], &[]), Vec::<String>::new());
    }

    #[test]
    fn declared_names_are_read_from_generated_files() {
        let names: Vec<String> = declared_names(
            "export type NoteId = string;\nexport type Page<T> = { items: Array<T>, };\n",
        )
        .collect();
        assert_eq!(names, vec!["NoteId", "Page"]);
    }
}
