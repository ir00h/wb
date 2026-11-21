use std::path::Path;

use crate::finding::{self, Finding, line_of};

const HOME_FIX: &str = "use a path inside the repository or an invented example";

fn unix_home_prefixes() -> [String; 2] {
    [["/Us", "ers/"].concat(), ["/ho", "me/"].concat()]
}

fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"._-".contains(&byte)
}

fn path_end(rest: &str) -> usize {
    rest.find(|c: char| c.is_whitespace() || "\"'`)".contains(c))
        .unwrap_or(rest.len())
}

fn unix_home_paths(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    for prefix in unix_home_prefixes() {
        for (at, _) in text.match_indices(&prefix) {
            let after_a_name = at > 0 && is_name_byte(bytes[at - 1]);
            let rest = &text[at + prefix.len()..];
            if !after_a_name && rest.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_') {
                found.push((at, at + prefix.len() + path_end(rest)));
            }
        }
    }
    found
}

fn separator_length(bytes: &[u8]) -> usize {
    match bytes {
        [b'\\', b'\\', ..] => 2,
        [b'\\' | b'/', ..] => 1,
        _ => 0,
    }
}

fn windows_home_paths(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let folder = ["us", "ers"].concat();
    let mut found = Vec::new();
    for (colon, _) in text.match_indices(':') {
        let Some(drive) = colon.checked_sub(1) else {
            continue;
        };
        if !bytes[drive].is_ascii_alphabetic()
            || (drive > 0 && bytes[drive - 1].is_ascii_alphanumeric())
        {
            continue;
        }
        let mut index = colon + 1;
        let first = separator_length(&bytes[index..]);
        index += first;
        let names_folder = bytes
            .get(index..index + folder.len())
            .is_some_and(|word| word.eq_ignore_ascii_case(folder.as_bytes()));
        if first == 0 || !names_folder {
            continue;
        }
        index += folder.len();
        let second = separator_length(&bytes[index..]);
        if second > 0
            && bytes
                .get(index + second)
                .is_some_and(u8::is_ascii_alphanumeric)
        {
            found.push((drive, index + second + path_end(&text[index + second..])));
        }
    }
    found
}

fn home_path_in_file_name(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').collect();
    let users = ["Us", "ers"].concat();
    let home = ["ho", "me"].concat();
    segments.first() == Some(&home.as_str()) && segments.len() > 2
        || segments
            .windows(3)
            .any(|window| window[0] == users && !window[1].is_empty())
}

pub fn check(root: &Path) -> Result<(), String> {
    let mut findings = Vec::new();
    for path in finding::tracked_files(root)? {
        if home_path_in_file_name(&path) {
            findings.push(Finding::whole(
                &path,
                format!("the file name holds a home folder path; {HOME_FIX}"),
            ));
        }
        if let Ok(text) = std::fs::read_to_string(root.join(&path)) {
            findings.extend(problems_in(&path, &text));
        }
    }
    finding::report(&findings)
}

fn problems_in(path: &str, text: &str) -> Vec<Finding> {
    let mut found: Vec<(usize, String)> = Vec::new();
    let windows = windows_home_paths(text);
    let unix = unix_home_paths(text)
        .into_iter()
        .filter(|(start, _)| !windows.iter().any(|(from, to)| from < start && start < to));
    for (start, end) in windows.iter().copied().chain(unix) {
        found.push((
            start,
            format!("home folder path `{}`; {HOME_FIX}", &text[start..end]),
        ));
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
        let first_label = domain.split('.').next().unwrap_or_default();
        let is_scale_suffix = first_label.len() > 1
            && first_label.ends_with('x')
            && first_label[..first_label.len() - 1]
                .chars()
                .all(|c| c.is_ascii_digit());
        let is_remote_path = bytes.get(at + 1 + domain.len()) == Some(&b':');
        let top_level = domain.rsplit_once('.').map(|(_, label)| label);
        let looks_real = start < at
            && !is_scale_suffix
            && !is_remote_path
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
                    windows
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
    fn escaped_and_lower_case_windows_home_paths_are_reported() {
        let escaped = ["C:\\\\Us", "ers\\\\gardener\\\\notes"].concat();
        let lower = ["c:\\us", "ers\\gardener"].concat();
        let forward = ["D:/Us", "ers/gardener"].concat();
        let text = format!("\"{escaped}\"\n{lower}\n{forward}\n");
        let lines: Vec<String> = messages(&text)
            .into_iter()
            .map(|message| message.split(": ").next().unwrap_or_default().to_owned())
            .collect();
        assert_eq!(lines, vec!["notes.md:1", "notes.md:2", "notes.md:3"]);
        assert!(messages(&text)[0].contains(&format!("`{escaped}`")));
    }

    #[test]
    fn a_home_path_in_a_file_name_is_found() {
        assert!(super::home_path_in_file_name(
            &["docs/Us", "ers/gardener/notes.txt"].concat()
        ));
        assert!(super::home_path_in_file_name(
            &["ho", "me/gardener/notes.txt"].concat()
        ));
        assert!(!super::home_path_in_file_name(
            "window/src/ui/home/Page.tsx"
        ));
    }

    #[test]
    fn icons_urls_and_remotes_are_not_reported() {
        let text = format!(
            "\"icons/128x128@2x.png\"\nhttps://example.org{}page\ngit clone git@example.org:garden/whiteboard.git\n",
            ["/ho", "me/"].concat()
        );
        assert_eq!(messages(&text), Vec::<String>::new());
    }

    #[test]
    fn package_specifiers_and_scopes_are_not_addresses() {
        let text = "\"@tauri-apps/api\": \"2.12.5\"\nreact-dom@19.3.1\n'@vitejs/plugin-react@6.3.0(vite@8.3.8)'\n#[serde(rename = \"x\")]\n";
        assert_eq!(messages(text), Vec::<String>::new());
    }
}
