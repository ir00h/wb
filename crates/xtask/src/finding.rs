use std::fmt;
use std::path::Path;

use crate::process;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: String,
    pub line: Option<usize>,
    pub message: String,
}

impl Finding {
    pub fn new(path: &str, line: usize, message: impl Into<String>) -> Finding {
        Finding {
            path: path.to_owned(),
            line: Some(line),
            message: message.into(),
        }
    }

    pub fn whole(path: &str, message: impl Into<String>) -> Finding {
        Finding {
            path: path.to_owned(),
            line: None,
            message: message.into(),
        }
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(formatter, "{}:{line}: {}", self.path, self.message),
            None => write!(formatter, "{}: {}", self.path, self.message),
        }
    }
}

pub fn report(findings: &[Finding]) -> Result<(), String> {
    if findings.is_empty() {
        return Ok(());
    }
    let lines: Vec<String> = findings.iter().map(ToString::to_string).collect();
    Err(lines.join("\n"))
}

pub fn line_of(text: &str, offset: usize) -> usize {
    text.as_bytes()[..offset.min(text.len())]
        .split(|&byte| byte == b'\n')
        .count()
}

pub fn tracked_files(root: &Path) -> Result<Vec<String>, String> {
    let listing = process::output(
        root,
        &[
            "git",
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
    )?;
    let mut paths: Vec<String> = listing
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect();
    paths.sort_unstable();
    paths.dedup();
    Ok(paths)
}
