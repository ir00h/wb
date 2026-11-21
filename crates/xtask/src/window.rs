use std::fs;
use std::path::Path;

use crate::process;

const LOCKFILE: &str = "pnpm-lock.yaml";
const INSTALLED_LOCKFILE: &str = "node_modules/.pnpm/lock.yaml";

pub fn ensure_packages(root: &Path) -> Result<(), String> {
    let wanted = fs::read(root.join(LOCKFILE))
        .map_err(|error| format!("cannot read {LOCKFILE}: {error}"))?;
    let installed = fs::read(root.join(INSTALLED_LOCKFILE)).ok();
    if installed.as_deref() == Some(wanted.as_slice()) {
        return Ok(());
    }
    process::output(root, &["pnpm", "install", "--frozen-lockfile"]).map(|_| ())
}

pub fn check(root: &Path) -> Result<(), String> {
    ensure_packages(root)?;
    process::run_all(
        root,
        &[
            &["pnpm", "exec", "biome", "check", "."],
            &["pnpm", "exec", "tsc", "--noEmit", "-p", "tsconfig.json"],
            &[
                "pnpm",
                "exec",
                "tsc",
                "--noEmit",
                "-p",
                "tsconfig.node.json",
            ],
            &["pnpm", "exec", "tsc", "--noEmit", "-p", "tsconfig.e2e.json"],
            &["pnpm", "exec", "vitest", "run"],
            &["pnpm", "exec", "vite", "build"],
        ],
    )
}

pub fn layers(root: &Path) -> Result<(), String> {
    ensure_packages(root)?;
    process::output(
        root,
        &[
            "pnpm",
            "exec",
            "depcruise",
            "window/src",
            "--config",
            ".dependency-cruiser.json",
        ],
    )
    .map(|_| ())
}
