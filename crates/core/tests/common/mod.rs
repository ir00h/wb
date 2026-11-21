#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use wb_core::{Board, Workspace};
use wb_model::{
    BodyChange, Change, NoteCard, NoteDraft, NoteEdit, NoteId, Op, Outcome, Parsed, parse,
};

pub const ROLE: &str = "WB_TEST_ROLE";
const PATIENCE: Duration = Duration::from_secs(30);

static SCRATCH_FOLDERS: AtomicU32 = AtomicU32::new(0);

pub struct Scratch {
    path: PathBuf,
}

impl Scratch {
    pub fn new(name: &str) -> Scratch {
        let count = SCRATCH_FOLDERS.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("wb-core-{name}-{}-{count}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("scratch folder");
        Scratch { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn workspace(&self) -> Workspace {
        Workspace::open(&self.path).expect("workspace")
    }

    pub fn garden(&self) -> Board {
        let workspace = self.workspace();
        let garden = workspace.add_board("Garden", None).expect("board");
        workspace.board(&garden.id).expect("board handle")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub fn open_board(workspace: &Path, board: &str) -> Board {
    let id = board.parse().expect("board id");
    Workspace::open(workspace)
        .expect("workspace")
        .board(&id)
        .expect("board handle")
}

pub fn add_note(board: &Board, title: &str, body: &str) -> NoteCard {
    let draft = NoteDraft {
        title: title.to_owned(),
        body: body.to_owned(),
        ..NoteDraft::default()
    };
    match board.apply(Op::AddNote { draft }).expect("add note") {
        Outcome::Applied { note } => note,
        other => panic!("adding a note answered {other:?}"),
    }
}

pub fn change(base: Option<&str>, to: &str) -> Change<String> {
    Change {
        base: base.map(str::to_owned),
        to: to.to_owned(),
    }
}

pub fn set_body(note: &NoteId, base: Option<&str>, to: &str) -> Op {
    Op::EditNote {
        note: note.clone(),
        edit: NoteEdit {
            title: None,
            body: Some(BodyChange::Set(change(base, to))),
        },
    }
}

pub fn set_title(note: &NoteId, base: Option<&str>, to: &str) -> Op {
    Op::EditNote {
        note: note.clone(),
        edit: NoteEdit {
            title: Some(change(base, to)),
            body: None,
        },
    }
}

pub fn append(note: &NoteId, text: &str) -> Op {
    Op::EditNote {
        note: note.clone(),
        edit: NoteEdit {
            title: None,
            body: Some(BodyChange::Append {
                text: text.to_owned(),
            }),
        },
    }
}

pub fn add_section(name: &str) -> Op {
    Op::AddSection {
        name: name.to_owned(),
        parent: None,
        note: String::new(),
        hue: None,
    }
}

pub fn files_holding(folder: &Path, id: &NoteId) -> Vec<String> {
    let mut names = Vec::new();
    for entry in fs::read_dir(folder).expect("folder") {
        let path = entry.expect("entry").path();
        if path.extension().is_none_or(|extension| extension != "md") {
            continue;
        }
        let text = fs::read_to_string(&path).expect("note file");
        if matches!(parse(&text), Parsed::Note(note) if &note.id == id) {
            names.push(
                path.file_name()
                    .expect("file name")
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    names.sort();
    names
}

pub fn tree_bytes(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(folder) = pending.pop() {
        for entry in fs::read_dir(&folder).expect("folder") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .expect("inside the root")
                .to_string_lossy()
                .replace('\\', "/");
            files.insert(relative, fs::read(&path).expect("file"));
        }
    }
    files
}

pub fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("target folder");
    for entry in fs::read_dir(from).expect("source folder") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copy");
        }
    }
}

pub fn wait_for(what: &str, mut happened: impl FnMut() -> bool) {
    let started = Instant::now();
    while !happened() {
        assert!(started.elapsed() < PATIENCE, "gave up waiting for {what}");
        thread::sleep(Duration::from_millis(1));
    }
}

pub fn spawn_role(test: &str, role: &str, settings: &[(&str, &str)]) -> Child {
    let mut command = Command::new(std::env::current_exe().expect("test binary"));
    command
        .args([test, "--exact", "--nocapture", "--test-threads=1"])
        .env(ROLE, role)
        .stdout(Stdio::null());
    for (name, value) in settings {
        command.env(name, value);
    }
    command.spawn().expect("child process")
}

pub fn setting(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("the setting {name} is missing"))
}
