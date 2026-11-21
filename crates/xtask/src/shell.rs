use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::process;

const SHELL_PACKAGE: &str = "whiteboard";

const CHANNEL_MARKERS: [&str; 5] = [
    "AUTOMATION_LISTENING",
    "WB_AUTOMATION_PORT",
    "WB_AUTOMATION_BACKGROUND",
    "settle_evaluation",
    "_setWindowOcclusionDetectionEnabled",
];

fn build(root: &Path, features: Option<&str>) -> Result<PathBuf, String> {
    let mut argv = vec![
        "cargo",
        "build",
        "--package",
        SHELL_PACKAGE,
        "--locked",
        "--message-format",
        "json-render-diagnostics",
    ];
    if let Some(features) = features {
        argv.extend(["--features", features]);
    }
    let messages = process::output(root, &argv)?;
    executable_in(&messages).ok_or_else(|| {
        format!(
            "`{}` reported no {SHELL_PACKAGE} executable",
            argv.join(" ")
        )
    })
}

fn executable_in(messages: &str) -> Option<PathBuf> {
    messages
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|message| {
            message["reason"] == "compiler-artifact" && message["target"]["name"] == SHELL_PACKAGE
        })
        .find_map(|message| message["executable"].as_str().map(PathBuf::from))
}

pub fn default_clean(root: &Path) -> Result<(), String> {
    let shell = build(root, None)?;
    let binary = std::fs::read(&shell)
        .map_err(|error| format!("cannot read {}: {error}", shell.display()))?;
    let leaked = leaked_markers(&binary);
    if leaked.is_empty() {
        return Ok(());
    }
    Err(format!(
        "the default {SHELL_PACKAGE} binary contains automation channel strings: {}; keep every part of the channel behind #[cfg(feature = \"automation\")]",
        leaked.join(", ")
    ))
}

fn leaked_markers(binary: &[u8]) -> Vec<&'static str> {
    CHANNEL_MARKERS
        .into_iter()
        .filter(|marker| {
            binary
                .windows(marker.len())
                .any(|window| window == marker.as_bytes())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{executable_in, leaked_markers};
    use std::path::PathBuf;

    #[test]
    fn a_marker_anywhere_in_the_binary_is_reported() {
        let binary = b"\x00\x01prefix AUTOMATION_LISTENING suffix\x00settle_evaluation\xff";
        assert_eq!(
            leaked_markers(binary),
            vec!["AUTOMATION_LISTENING", "settle_evaluation"]
        );
        assert_eq!(leaked_markers(b"Whiteboard"), Vec::<&str>::new());
    }

    #[test]
    fn the_shell_executable_is_read_from_cargo_messages() {
        let messages = [
            r#"{"reason":"compiler-artifact","target":{"name":"wb-core"},"executable":null}"#,
            "not json",
            r#"{"reason":"compiler-artifact","target":{"name":"whiteboard"},"executable":"/build/debug/whiteboard"}"#,
            r#"{"reason":"build-finished","success":true}"#,
        ]
        .join("\n");
        assert_eq!(
            executable_in(&messages),
            Some(PathBuf::from("/build/debug/whiteboard"))
        );
    }
}
