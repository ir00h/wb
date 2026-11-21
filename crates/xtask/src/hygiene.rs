use std::path::Path;

use crate::finding::{self, Finding, line_of};

fn home_folder_prefixes() -> [String; 3] {
    [
        ["/Us", "ers/"].concat(),
        ["/ho", "me/"].concat(),
        [":\\Us", "ers\\"].concat(),
    ]
}

pub fn check(root: &Path) -> Result<(), String> {
    let mut findings = Vec::new();
    for path in finding::tracked_files(root)? {
        if let Ok(text) = std::fs::read_to_string(root.join(&path)) {
            findings.extend(problems_in(&path, &text));
        }
    }
    finding::report(&findings)
}

fn problems_in(path: &str, text: &str) -> Vec<Finding> {
    let mut found: Vec<(usize, String)> = Vec::new();
    for prefix in home_folder_prefixes() {
        for (at, _) in text.match_indices(&prefix) {
            let rest = &text[at + prefix.len()..];
            if rest.starts_with(|c: char| c.is_ascii_alphanumeric()) {
                let end = rest
                    .find(|c: char| c.is_whitespace() || "\"'`)".contains(c))
                    .unwrap_or(rest.len());
                found.push((
                    at,
                    format!(
                        "home folder path `{prefix}{}`; use a path inside the repository or an invented example",
                        &rest[..end]
                    ),
                ));
            }
        }
    }
    for (start, end) in email_addresses(text) {
        found.push((
            start,
            format!(
                "e-mail address `{}`; remove it, tracked files carry no addresses",
                &text[start..end]
            ),
        ));
    }
    found.sort();
    found
        .into_iter()
        .map(|(at, message)| Finding::new(path, line_of(text, at), message))
        .collect()
}

fn is_local_part(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"._%+-".contains(&byte)
}

fn is_domain_part(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-'
}

fn email_addresses(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    for (at, _) in text.match_indices('@') {
        let start = bytes[..at]
            .iter()
            .rposition(|&byte| !is_local_part(byte))
            .map_or(0, |position| position + 1);
        let end = bytes[at + 1..]
            .iter()
            .position(|&byte| !is_domain_part(byte))
            .map_or(bytes.len(), |position| at + 1 + position);
        let domain = text[at + 1..end].trim_end_matches('.');
        let top_level = domain.rsplit_once('.').map(|(_, label)| label);
        let looks_real = start < at
            && top_level.is_some_and(|label| {
                label.len() >= 2 && label.chars().all(|c| c.is_ascii_alphabetic())
            });
        if looks_real {
            found.push((start, at + 1 + domain.len()));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::problems_in;

    fn messages(text: &str) -> Vec<String> {
        problems_in("notes.md", text)
            .into_iter()
            .map(|finding| finding.to_string())
            .collect()
    }

    #[test]
    fn home_folder_paths_on_every_platform_are_reported() {
        let mac = ["/Us", "ers/gardener/notes"].concat();
        let linux = ["/ho", "me/gardener"].concat();
        let windows = ["C:\\Us", "ers\\gardener\\notes"].concat();
        let text = format!("see {mac}\nand {linux}\nand {windows}\n");
        assert_eq!(
            messages(&text),
            vec![
                format!(
                    "notes.md:1: home folder path `{mac}`; use a path inside the repository or an invented example"
                ),
                format!(
                    "notes.md:2: home folder path `{linux}`; use a path inside the repository or an invented example"
                ),
                format!(
                    "notes.md:3: home folder path `{}`; use a path inside the repository or an invented example",
                    windows.trim_start_matches('C')
                ),
            ]
        );
    }

    #[test]
    fn email_addresses_are_reported() {
        let address = ["gardener", "@", "example.org"].concat();
        assert_eq!(
            messages(&format!("write to {address}.\n")),
            vec![format!(
                "notes.md:1: e-mail address `{address}`; remove it, tracked files carry no addresses"
            )]
        );
    }

    #[test]
    fn package_specifiers_and_scopes_are_not_addresses() {
        let text = "\"@tauri-apps/api\": \"2.12.5\"\nreact-dom@19.3.1\n'@vitejs/plugin-react@6.3.0(vite@8.3.8)'\n#[serde(rename = \"x\")]\n";
        assert_eq!(messages(text), Vec::<String>::new());
    }
}
