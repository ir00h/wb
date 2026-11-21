use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use crate::{bindings, comments, commits, hygiene, layers, process, shell, window, window_rules};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Comments,
    Layers,
    Bindings,
    Commits,
    Hygiene,
    Rust,
    Window,
    ShellDefaultClean,
}

impl Step {
    pub const ALL: &'static [Step] = &[
        Step::Comments,
        Step::Layers,
        Step::Bindings,
        Step::Commits,
        Step::Hygiene,
        Step::Rust,
        Step::Window,
        Step::ShellDefaultClean,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Step::Comments => "comments",
            Step::Layers => "layers",
            Step::Bindings => "bindings",
            Step::Commits => "commits",
            Step::Hygiene => "hygiene",
            Step::Rust => "rust",
            Step::Window => "window",
            Step::ShellDefaultClean => "shell-default-clean",
        }
    }

    pub fn from_name(name: &str) -> Option<Step> {
        Step::ALL.iter().copied().find(|step| step.name() == name)
    }

    fn run(self, root: &Path) -> Result<(), String> {
        match self {
            Step::Comments => comments::check(root),
            Step::Layers => all([
                layers::check(root),
                window::layers(root),
                window_rules::check(root),
            ]),
            Step::Bindings => bindings::check(root),
            Step::Commits => commits::check(root),
            Step::Hygiene => hygiene::check(root),
            Step::Rust => rust(root),
            Step::Window => window::check(root),
            Step::ShellDefaultClean => shell::default_clean(root),
        }
    }
}

fn all<const N: usize>(outcomes: [Result<(), String>; N]) -> Result<(), String> {
    let reports: Vec<String> = outcomes.into_iter().filter_map(Result::err).collect();
    if reports.is_empty() {
        Ok(())
    } else {
        Err(reports.join("\n"))
    }
}

fn rust(root: &Path) -> Result<(), String> {
    let commands = rust_commands();
    let commands: Vec<&[&str]> = commands.iter().map(Vec::as_slice).collect();
    process::run_all(root, &commands)
}

fn rust_commands() -> Vec<Vec<&'static str>> {
    let mut commands = vec![vec!["cargo", "fmt", "--all", "--check"]];
    commands.extend(
        layers::packages()
            .map(|package| vec!["cargo", "check", "--locked", "--quiet", "-p", package]),
    );
    commands.push(vec![
        "cargo",
        "clippy",
        "--workspace",
        "--all-targets",
        "--locked",
        "--",
        "-D",
        "warnings",
    ]);
    commands.push(vec![
        "cargo",
        "clippy",
        "--package",
        "whiteboard",
        "--all-targets",
        "--features",
        "automation",
        "--locked",
        "--",
        "-D",
        "warnings",
    ]);
    commands.push(vec!["cargo", "test", "--workspace", "--locked", "--quiet"]);
    commands
}

pub fn run(root: &Path, steps: &[Step]) -> ExitCode {
    let mut failed = Vec::new();
    for &step in steps {
        let started = Instant::now();
        let outcome = step.run(root);
        let seconds = started.elapsed().as_secs_f64();
        let verdict = if outcome.is_ok() { "pass" } else { "FAIL" };
        println!("{verdict}  {:<19} {seconds:>7.2}s", step.name());
        if let Err(report) = outcome {
            failed.push((step, report));
        }
    }
    if failed.is_empty() {
        return ExitCode::SUCCESS;
    }
    for (step, report) in &failed {
        println!("\n---- {} ----\n{}", step.name(), report.trim_end());
    }
    let names: Vec<&str> = failed.iter().map(|(step, _)| step.name()).collect();
    println!("\nfailed: {}", names.join(", "));
    ExitCode::FAILURE
}

#[cfg(test)]
mod tests {
    use super::rust_commands;
    use crate::layers;

    #[test]
    fn every_package_in_the_layer_table_is_built_alone() {
        let built_alone: Vec<&str> = rust_commands()
            .iter()
            .filter_map(|command| match command.as_slice() {
                ["cargo", "check", "--locked", "--quiet", "-p", package] => Some(*package),
                _ => None,
            })
            .collect();
        assert_eq!(built_alone, layers::packages().collect::<Vec<_>>());
    }
}
