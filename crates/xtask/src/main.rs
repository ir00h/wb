mod check;
mod process;

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: cargo xtask check [step]\nsteps: ";

fn repository_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .ancestors()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or(manifest_dir)
}

fn print_usage() -> ExitCode {
    let names: Vec<&str> = check::Step::ALL.iter().map(|step| step.name()).collect();
    eprintln!("{USAGE}{}", names.join(", "));
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let root = repository_root();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["check"] => check::run(&root, check::Step::ALL),
        ["check", name] => match check::Step::from_name(name) {
            Some(step) => check::run(&root, &[step]),
            None => print_usage(),
        },
        _ => print_usage(),
    }
}
