use std::path::Path;
use std::process::Command;

pub fn output(root: &Path, argv: &[&str]) -> Result<String, String> {
    let (program, arguments) = argv
        .split_first()
        .ok_or_else(|| "empty command".to_owned())?;
    let output = Command::new(program)
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|error| format!("`{}` could not start: {error}", argv.join(" ")))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if output.status.success() {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!(
        "`{}` failed ({}):\n{stdout}{stderr}",
        argv.join(" "),
        output.status
    ))
}

pub fn run_all(root: &Path, commands: &[&[&str]]) -> Result<(), String> {
    let failures: Vec<String> = commands
        .iter()
        .filter_map(|argv| output(root, argv).err())
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}
