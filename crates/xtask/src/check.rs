use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use crate::{comments, process};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Comments,
    Rust,
}

impl Step {
    pub const ALL: &'static [Step] = &[Step::Comments, Step::Rust];

    pub fn name(self) -> &'static str {
        match self {
            Step::Comments => "comments",
            Step::Rust => "rust",
        }
    }

    pub fn from_name(name: &str) -> Option<Step> {
        Step::ALL.iter().copied().find(|step| step.name() == name)
    }

    fn run(self, root: &Path) -> Result<(), String> {
        match self {
            Step::Comments => comments::check(root),
            Step::Rust => rust(root),
        }
    }
}

fn rust(root: &Path) -> Result<(), String> {
    process::run_all(
        root,
        &[
            &["cargo", "fmt", "--all", "--check"],
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
