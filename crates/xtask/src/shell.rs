use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::{process, window};

const SHELL_PACKAGE: &str = "whiteboard";
const CLI_PACKAGE: &str = "wb";
const AUTOMATION_FEATURES: &str = "automation,tauri/custom-protocol";
const JOURNEY_RUNNER: &str = "e2e/run.ts";
const AUTOMATION_EXECUTABLE: &str = "whiteboard-automation";

const CHANNEL_MARKERS: [&str; 11] = [
    "AUTOMATION_LISTENING",
    "WB_AUTOMATION_PORT",
    "WB_AUTOMATION_BACKGROUND",
    "WB_AUTOMATION_RUNNER",
    "settle_evaluation",
    "_setWindowOcclusionDetectionEnabled",
    "OperationCount",
    "/evaluate",
    "/snapshot",
    "/applied",
    "/terminate",
];

fn build(root: &Path, package: &str, features: Option<&str>) -> Result<PathBuf, String> {
    let mut argv = vec![
        "cargo",
        "build",
        "--package",
        package,
        "--locked",
        "--message-format",
        "json-render-diagnostics",
    ];
    if let Some(features) = features {
        argv.extend(["--features", features]);
    }
    let messages = process::output(root, &argv)?;
    executable_in(&messages, package)
        .ok_or_else(|| format!("`{}` reported no {package} executable", argv.join(" ")))
}

fn executable_in(messages: &str, package: &str) -> Option<PathBuf> {
    messages
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|message| {
            message["reason"] == "compiler-artifact" && message["target"]["name"] == package
        })
        .find_map(|message| message["executable"].as_str().map(PathBuf::from))
}

pub fn journeys(root: &Path) -> Result<(), String> {
    window::ensure_packages(root)?;
    process::output(root, &["pnpm", "exec", "vite", "build"])?;
    let wb = build(root, CLI_PACKAGE, None)?;
    let built = build(root, SHELL_PACKAGE, Some(AUTOMATION_FEATURES))?;
    let shell = built.with_file_name(AUTOMATION_EXECUTABLE);
    copy_to_new_file(&built, &shell).map_err(|error| {
        format!(
            "cannot copy {} to {}: {error}",
            built.display(),
            shell.display()
        )
    })?;
    let output = Command::new("node")
        .arg(JOURNEY_RUNNER)
        .env("WB_E2E_SHELL", &shell)
        .env("WB_E2E_WB", &wb)
        .current_dir(root)
        .output()
        .map_err(|error| format!("`node {JOURNEY_RUNNER}` could not start: {error}"))?;
    print!("{}", String::from_utf8_lossy(&output.stdout));
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "`node {JOURNEY_RUNNER}` failed ({}); the journey lines above say why\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    ))
}

fn copy_to_new_file(from: &Path, to: &Path) -> std::io::Result<u64> {
    match std::fs::remove_file(to) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    std::fs::copy(from, to)
}

pub fn default_clean(root: &Path) -> Result<(), String> {
    let shell = build(root, SHELL_PACKAGE, None)?;
    let binary = std::fs::read(&shell)
        .map_err(|error| format!("cannot read {}: {error}", shell.display()))?;
    let leaked = leaked_markers(&binary);
    if leaked.is_empty() {
        return Ok(());
    }
    Err(format!(
        "the default {SHELL_PACKAGE} binary contains automation channel strings: {}; keep every part of the channel behind #[cfg(feature = \"automation\")]. This step reads the binary's bytes for the channel's variables, routes and type names; state kept without any of them, such as a count no route serves, is not seen",
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
            executable_in(&messages, "whiteboard"),
            Some(PathBuf::from("/build/debug/whiteboard"))
        );
    }
}
