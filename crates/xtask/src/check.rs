use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use crate::{bindings, comments, commits, hygiene, layers, process, window};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Comments,
    Layers,
    Bindings,
    Commits,
    Hygiene,
    Rust,
    Window,
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
        }
    }

    pub fn from_name(name: &str) -> Option<Step> {
        Step::ALL.iter().copied().find(|step| step.name() == name)
    }

    fn run(self, root: &Path) -> Result<(), String> {
        match self {
            Step::Comments => comments::check(root),
            Step::Layers => both(layers::check(root), window::layers(root)),
            Step::Bindings => bindings::check(root),
            Step::Commits => commits::check(root),
            Step::Hygiene => hygiene::check(root),
            Step::Rust => rust(root),
            Step::Window => window::check(root),
        }
    }
}

fn both(rust: Result<(), String>, typescript: Result<(), String>) -> Result<(), String> {
    let reports: Vec<String> = [rust, typescript]
        .into_iter()
        .filter_map(Result::err)
        .collect();
    if reports.is_empty() {
        Ok(())
    } else {
        Err(reports.join("\n"))
    }
}

fn rust(root: &Path) -> Result<(), String> {
    process::run_all(
        root,
        &[
            &["cargo", "fmt", "--all", "--check"],
            &["cargo", "check", "--locked", "--quiet", "-p", "wb-model"],
            &["cargo", "check", "--locked", "--quiet", "-p", "wb-core"],
            &["cargo", "check", "--locked", "--quiet", "-p", "wb"],
            &["cargo", "check", "--locked", "--quiet", "-p", "whiteboard"],
            &[
                "cargo",
                "clippy",
                "--workspace",
                "--all-targets",
                "--locked",
                "--",
                "-D",
                "warnings",
            ],
            &["cargo", "test", "--workspace", "--locked", "--quiet"],
        ],
    )
}

pub fn run(root: &Path, steps: &[Step]) -> ExitCode {
    let mut failed = Vec::new();
    for &step in steps {
        let started = Instant::now();
        let outcome = step.run(root);
        let seconds = started.elapsed().as_secs_f64();
        let verdict = if outcome.is_ok() { "pass" } else { "FAIL" };
        println!("{verdict}  {:<9} {seconds:>6.2}s", step.name());
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
