use std::path::Path;

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

use crate::finding::{self, Finding, line_of};

const SOURCE: &str = "window/src/";
const ENTRY: &str = "window/src/main.tsx";
const LAYERS: [&str; 5] = ["bindings", "domain", "bridge", "state", "ui"];
const PLATFORM_LAYER: &str = "bridge";
const PLATFORM_GLOBAL: &str = "__TAURI";

pub fn check(root: &Path) -> Result<(), String> {
    let mut findings = Vec::new();
    for path in finding::tracked_files(root)? {
        let Some(inside) = path.strip_prefix(SOURCE) else {
            continue;
        };
        let layer = layer_of(inside);
        if layer.is_none() && path != ENTRY {
            findings.push(outside_the_layers(&path));
        }
        if SourceType::from_path(&path).is_ok()
            && let Ok(text) = std::fs::read_to_string(root.join(&path))
        {
            findings.extend(script_findings(&path, layer, &text));
        }
    }
    finding::report(&findings)
}

fn layer_of(inside: &str) -> Option<&'static str> {
    let (folder, _) = inside.split_once('/')?;
    LAYERS.into_iter().find(|layer| *layer == folder)
}

fn outside_the_layers(path: &str) -> Finding {
    Finding::whole(
        path,
        format!(
            "is outside the window layers; move it into one of {} under {SOURCE}, or into {ENTRY} if it starts the window",
            LAYERS.join(", ")
        ),
    )
}

fn is_one_string_literal(text: &str) -> bool {
    let bytes = text.as_bytes();
    let Some((&quote, rest)) = bytes.split_first() else {
        return false;
    };
    if quote != b'"' && quote != b'\'' {
        return false;
    }
    let mut index = 0;
    while index < rest.len() {
        match rest[index] {
            b'\\' => index += 1,
            byte if byte == quote => return index == rest.len() - 1,
            _ => {}
        }
        index += 1;
    }
    false
}

fn script_findings(path: &str, layer: Option<&str>, text: &str) -> Vec<Finding> {
    let mut found: Vec<(usize, &str)> = Vec::new();
    if layer != Some(PLATFORM_LAYER) {
        for (at, _) in text.match_indices(PLATFORM_GLOBAL) {
            found.push((
                at,
                "reaches the Tauri globals; only window/src/bridge talks to the desktop shell, call a bridge function through state",
            ));
        }
    }
    let Ok(source_type) = SourceType::from_path(path) else {
        return Vec::new();
    };
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, text, source_type).parse();
    for import in &parsed.module_record.dynamic_imports {
        let request =
            &text[import.module_request.start as usize..import.module_request.end as usize];
        if !is_one_string_literal(request.trim()) {
            found.push((
                import.span.start as usize,
                "imports a module whose name is computed, which the layer rules cannot follow; import a fixed path",
            ));
        }
    }
    for meta in &parsed.module_record.import_metas {
        let after = text[meta.end as usize..].trim_start();
        let after = after
            .strip_prefix("?.")
            .or_else(|| after.strip_prefix('.'))
            .map_or(after, str::trim_start);
        if after.starts_with("glob") || after.starts_with('[') {
            found.push((
                meta.start as usize,
                "imports files through `import.meta.glob`, which the layer rules cannot follow; import each module by a fixed path",
            ));
        }
    }
    found.sort_unstable();
    found
        .into_iter()
        .map(|(at, message)| Finding::new(path, line_of(text, at), message))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{layer_of, outside_the_layers, script_findings};

    fn messages(path: &str, layer: Option<&str>, text: &str) -> Vec<String> {
        script_findings(path, layer, text)
            .into_iter()
            .map(|finding| finding.to_string())
            .collect()
    }

    #[test]
    fn files_belong_to_one_of_the_five_layers() {
        assert_eq!(layer_of("ui/App.tsx"), Some("ui"));
        assert_eq!(layer_of("bridge/deep/x.ts"), Some("bridge"));
        assert_eq!(layer_of("features/everything.ts"), None);
        assert_eq!(layer_of("loose.ts"), None);
        assert_eq!(
            outside_the_layers("window/src/loose.ts").to_string(),
            "window/src/loose.ts: is outside the window layers; move it into one of bindings, domain, bridge, state, ui under window/src/, or into window/src/main.tsx if it starts the window"
        );
    }

    #[test]
    fn the_tauri_globals_are_reached_only_from_the_bridge() {
        let text = "export const a = () => (globalThis as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__;\nexport const b = (window as unknown as Record<string, unknown>)[\"__TAURI__\"];\n";
        let found = messages("window/src/ui/a.ts", Some("ui"), text);
        assert_eq!(found.len(), 3);
        assert!(found[0].starts_with("window/src/ui/a.ts:1: reaches the Tauri globals"));
        assert_eq!(
            messages("window/src/bridge/a.ts", Some("bridge"), text),
            Vec::<String>::new()
        );
    }

    #[test]
    fn computed_imports_and_glob_imports_are_refused() {
        let text = "const name = \"platform\";\nexport const a = () => import(`../bridge/${name}.ts`);\nexport const b = () => import(\"../bridge/\" + name);\nexport const c = import.meta.glob(\"../bridge/*.ts\", { eager: true });\nexport const d = () => import(\"../state/app\");\nexport const e = import.meta.env.MODE;\n";
        let lines: Vec<String> = messages("window/src/ui/a.ts", Some("ui"), text)
            .into_iter()
            .map(|message| message.split(": ").next().unwrap_or_default().to_owned())
            .collect();
        assert_eq!(
            lines,
            vec![
                "window/src/ui/a.ts:2",
                "window/src/ui/a.ts:3",
                "window/src/ui/a.ts:4"
            ]
        );
    }
}
