#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use serde_json::Value;

pub const WORKSPACE_ENV: &str = "WHITEBOARD_WORKSPACE";
pub const BOARD_ENV: &str = "WB_BOARD";
const HOME_ENV: &str = "HOME";
const TMP: &str = "<tmp>";
const SHELL: &str = "/bin/sh";
const WAIT_FOR_THE_START: &str = r#": > "$1"; while [ ! -e "$2" ]; do :; done; shift 2; exec "$@""#;
const READY_DEADLINE: Duration = Duration::from_secs(20);
const READY_POLL: Duration = Duration::from_millis(1);

static SCRATCH_FOLDERS: AtomicU32 = AtomicU32::new(0);

pub struct Scratch {
    path: PathBuf,
}

#[derive(Debug)]
pub struct Run {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

impl Run {
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.stdout).unwrap_or_else(|error| {
            panic!(
                "stdout is not JSON ({error}):\n{}\n{}",
                self.stdout, self.stderr
            )
        })
    }

    pub fn ok(self) -> Run {
        assert_eq!(self.code, 0, "the command failed:\n{}", self.stderr);
        self
    }
}

impl Scratch {
    pub fn new(name: &str) -> Scratch {
        let count = SCRATCH_FOLDERS.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("wb-cli-{name}-{}-{count}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("scratch folder");
        Scratch {
            path: path.canonicalize().expect("scratch path"),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn workspace(&self) -> PathBuf {
        self.path.join("ws")
    }

    pub fn wb(&self, args: &[&str]) -> Run {
        self.wb_in(&self.workspace(), args, None)
    }

    pub fn wb_with_stdin(&self, args: &[&str], stdin: &str) -> Run {
        self.wb_in(&self.workspace(), args, Some(stdin))
    }

    pub fn wb_in(&self, workspace: &Path, args: &[&str], stdin: Option<&str>) -> Run {
        let mut command = self.command();
        command.arg("--workspace").arg(workspace).args(args);
        run(command, stdin)
    }

    pub fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_wb"));
        command
            .current_dir(&self.path)
            .env(HOME_ENV, &self.path)
            .env_remove(WORKSPACE_ENV)
            .env_remove(BOARD_ENV);
        command
    }

    pub fn start_together(&self, calls: &[Vec<String>]) -> Vec<Run> {
        let gate = self.path.join("start-together");
        fs::create_dir_all(&gate).expect("the start folder");
        let go = gate.join("go");
        let children: Vec<_> = calls
            .iter()
            .enumerate()
            .map(|(number, args)| {
                let mut command = self.command();
                command.arg("--workspace").arg(self.workspace()).args(args);
                let mut waiting = Command::new(SHELL);
                waiting
                    .current_dir(&self.path)
                    .env_clear()
                    .args(["-c", WAIT_FOR_THE_START, "wait"])
                    .arg(gate.join(format!("ready-{number}")))
                    .arg(&go)
                    .arg(command.get_program())
                    .args(command.get_args());
                std::thread::spawn(move || run(waiting, None))
            })
            .collect();
        let started = Instant::now();
        while fs::read_dir(&gate).expect("the start folder").count() < calls.len() {
            assert!(
                started.elapsed() < READY_DEADLINE,
                "not every writer got ready to start"
            );
            std::thread::sleep(READY_POLL);
        }
        fs::write(&go, "").expect("the start signal");
        children
            .into_iter()
            .map(|child| child.join().expect("a writer"))
            .collect()
    }

    pub fn normalise(&self, text: &str, ids: &[(&str, &str)]) -> String {
        let mut text = text.replace(&self.path.display().to_string(), TMP);
        for (name, id) in ids {
            text = text.replace(id, name);
        }
        text
    }

    pub fn tree(&self) -> BTreeMap<String, String> {
        let mut files = BTreeMap::new();
        collect(&self.workspace(), &self.workspace(), &mut files);
        files
    }

    pub fn fingerprint(&self, relative: &str) -> (Vec<String>, u64) {
        let mut entries = Vec::new();
        let mut hasher = DefaultHasher::new();
        let root = self.workspace().join(relative);
        let mut folders = vec![root.clone()];
        while let Some(folder) = folders.pop() {
            let mut paths: Vec<PathBuf> = fs::read_dir(&folder)
                .expect("a folder of the board")
                .map(|entry| entry.expect("an entry").path())
                .collect();
            paths.sort();
            for path in paths {
                let name = path
                    .strip_prefix(&root)
                    .expect("inside the board")
                    .display()
                    .to_string();
                if path.is_dir() {
                    entries.push(format!("{name}/"));
                    folders.push(path);
                } else {
                    let bytes = fs::read(&path).expect("a file of the board");
                    name.hash(&mut hasher);
                    bytes.hash(&mut hasher);
                    entries.push(name);
                }
            }
        }
        entries.sort();
        (entries, hasher.finish())
    }

    pub fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.workspace().join(relative)).expect("file in the workspace")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub fn run(mut command: Command, stdin: Option<&str>) -> Run {
    command
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("the wb binary starts");
    if let Some(text) = stdin {
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(text.as_bytes())
            .expect("stdin written");
    }
    let Output {
        status,
        stdout,
        stderr,
    } = child.wait_with_output().expect("wb finishes");
    Run {
        stdout: String::from_utf8(stdout).expect("utf-8 stdout"),
        stderr: String::from_utf8(stderr).expect("utf-8 stderr"),
        code: status.code().expect("an exit code"),
    }
}

fn collect(root: &Path, folder: &Path, files: &mut BTreeMap<String, String>) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .strip_prefix(root)
            .expect("inside the workspace")
            .display()
            .to_string();
        if path.is_dir() {
            collect(root, &path, files);
        } else {
            files.insert(name, fs::read_to_string(&path).unwrap_or_default());
        }
    }
}

pub fn text_of(value: &Value, key: &str) -> String {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("`{key}` is a string in {value}"))
        .to_owned()
}

pub struct Garden {
    pub scratch: Scratch,
    pub beds: String,
    pub trays: String,
    pub spring: String,
    pub herbs: String,
}

impl Garden {
    pub fn new(name: &str) -> Garden {
        let scratch = Scratch::new(name);
        scratch.wb(&["board", "add", "garden"]).ok();
        let section = |args: &[&str]| {
            let mut all = vec!["--board", "garden", "section", "add", "--json"];
            all.extend_from_slice(args);
            text_of(&scratch.wb(&all).ok().json(), "id")
        };
        let beds = section(&["Beds"]);
        let trays = section(&["Seed  trays"]);
        let spring = section(&["Spring", "--parent", "Beds"]);
        let herbs = section(&["Herbs", "--parent", "Beds / Spring"]);
        Garden {
            scratch,
            beds,
            trays,
            spring,
            herbs,
        }
    }

    pub fn wb(&self, args: &[&str]) -> Run {
        let mut all = vec!["--board", "garden"];
        all.extend_from_slice(args);
        self.scratch.wb(&all)
    }

    pub fn wb_with_stdin(&self, args: &[&str], stdin: &str) -> Run {
        let mut all = vec!["--board", "garden"];
        all.extend_from_slice(args);
        self.scratch.wb_with_stdin(&all, stdin)
    }

    pub fn add_note(&self, args: &[&str]) -> String {
        let mut all = vec!["note", "add", "--json"];
        all.extend_from_slice(args);
        text_of(&self.wb(&all).ok().json(), "id")
    }

    pub fn ids(&self) -> Vec<(&'static str, &str)> {
        vec![
            ("sBEDS00", self.beds.as_str()),
            ("sTRAYS0", self.trays.as_str()),
            ("sSPRING", self.spring.as_str()),
            ("sHERBS0", self.herbs.as_str()),
        ]
    }

    pub fn normalise(&self, text: &str, notes: &[(&str, &str)]) -> String {
        let mut ids = self.ids();
        ids.extend_from_slice(notes);
        self.scratch.normalise(text, &ids)
    }

    pub fn set_default_section(&self, id: &str) {
        let path = self.scratch.workspace().join("garden/board.config.json");
        let mut config: Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("config")).expect("JSON");
        config["defaultSection"] = Value::from(id);
        fs::write(
            &path,
            serde_json::to_string_pretty(&config).expect("JSON") + "\n",
        )
        .expect("config written");
    }
}
