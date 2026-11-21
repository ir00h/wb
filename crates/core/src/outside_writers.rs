use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use wb_model::{
    BodyChange, CONFIG_FILE, Change, Error, NoteCard, NoteDraft, NoteEdit, NoteId, Op, Outcome,
};

use crate::lock::{BoardLocks, LOCKS_FOLDER, LockKey};
use crate::replace::{
    Created, Replaced, Target, create_new, hook::before_the_next_exchange, replace_if_unchanged,
};
use crate::{Board, TRASH_FOLDER, Workspace};

const KILLED_WRITER: &str = "WB_TEST_KILLED_WRITER";
const KILLED_WRITER_NOTE: &str = "WB_TEST_KILLED_WRITER_NOTE";
static SCRATCH_FOLDERS: AtomicU32 = AtomicU32::new(0);
const AT_ONCE: Duration = Duration::from_millis(50);

struct Garden {
    root: PathBuf,
    board: Board,
}

impl Garden {
    fn new(name: &str) -> Garden {
        let count = SCRATCH_FOLDERS.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "wb-core-outside-{name}-{}-{count}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("scratch folder");
        let workspace = Workspace::open(&root).expect("workspace");
        let garden = workspace.add_board("Garden", None).expect("board");
        let board = workspace.board(&garden.id).expect("board handle");
        Garden { root, board }
    }

    fn folder(&self) -> PathBuf {
        self.root.join("garden")
    }

    fn add_note(&self, title: &str, body: &str) -> NoteCard {
        let draft = NoteDraft {
            title: title.to_owned(),
            body: body.to_owned(),
            ..NoteDraft::default()
        };
        match self.board.apply(Op::AddNote { draft }).expect("add note") {
            Outcome::Applied { note } => note,
            other => panic!("adding a note answered {other:?}"),
        }
    }

    fn file_of(&self, note: &NoteCard) -> PathBuf {
        let name = self.board.find(&note.id).expect("note file").name;
        self.folder().join(name)
    }

    fn left_in_the_locks_folder(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.folder().join(LOCKS_FOLDER))
            .expect("locks folder")
            .map(|entry| entry.expect("entry").path())
            .filter(|path| path.extension().is_none_or(|extension| extension != "lock"))
            .map(|path| path.display().to_string())
            .collect();
        names.sort();
        names
    }
}

impl Drop for Garden {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn body_edit(note: &NoteId, body: BodyChange) -> Op {
    Op::EditNote {
        note: note.clone(),
        edit: NoteEdit {
            title: None,
            body: Some(body),
        },
    }
}

fn append(note: &NoteId, text: &str) -> Op {
    body_edit(
        note,
        BodyChange::Append {
            text: text.to_owned(),
        },
    )
}

fn set_body(note: &NoteId, base: &str, to: &str) -> Op {
    body_edit(
        note,
        BodyChange::Set(Change {
            base: Some(base.to_owned()),
            to: to.to_owned(),
        }),
    )
}

fn move_to_the_trash(file: &Path) -> PathBuf {
    let trash = file.parent().expect("board folder").join(TRASH_FOLDER);
    fs::create_dir_all(&trash).expect("trash folder");
    let trashed = trash.join(file.file_name().expect("file name"));
    fs::rename(file, &trashed).expect("move to the trash");
    trashed
}

#[test]
fn a_note_trashed_between_the_read_and_the_write_stays_in_the_trash() {
    let garden = Garden::new("trashed");
    let note = garden.add_note("Weed the paths", "Before the rain.");
    let file = garden.file_of(&note);
    let before = fs::read(&file).expect("note file");

    before_the_next_exchange(|file| {
        move_to_the_trash(file);
    });
    let answer = garden.board.apply(append(&note.id, "After the rain."));

    assert!(
        !file.exists(),
        "the note trashed during the edit is back on the board and reads:\n{}",
        fs::read_to_string(&file).unwrap_or_default()
    );
    assert_eq!(
        answer,
        Err(Error::NoteNotFound {
            note: note.id.clone()
        })
    );
    let trashed = garden
        .folder()
        .join(TRASH_FOLDER)
        .join(file.file_name().expect("file name"));
    assert_eq!(fs::read(trashed).expect("trashed file"), before);
    assert_eq!(garden.left_in_the_locks_folder(), Vec::<String>::new());
}

#[test]
fn an_append_keeps_text_another_program_wrote_between_the_read_and_the_write() {
    let garden = Garden::new("outside-append");
    let note = garden.add_note("Harvest log", "Monday: peas.");
    let file = garden.file_of(&note);
    let outside = fs::read_to_string(&file)
        .expect("note file")
        .replace("Monday: peas.", "Monday: peas and beans.");

    let written_outside = outside.clone();
    before_the_next_exchange(move |file| fs::write(file, written_outside).expect("outside edit"));
    let answer = garden.board.apply(append(&note.id, "Tuesday: kale."));

    let now = fs::read_to_string(&file).expect("note file");
    assert_eq!(
        now,
        outside.clone() + "\nTuesday: kale.\n",
        "the text another program wrote during the edit is gone; the file was\n{outside}"
    );
    assert!(matches!(
        answer,
        Ok(Outcome::Applied { note }) if note.body == "Monday: peas and beans.\n\nTuesday: kale."
    ));
    assert_eq!(garden.left_in_the_locks_folder(), Vec::<String>::new());
}

#[test]
fn an_edit_with_a_base_is_a_conflict_when_another_program_wrote_between_the_read_and_the_write() {
    let garden = Garden::new("outside-conflict");
    let note = garden.add_note("Tomato plan", "Six plants.");
    let file = garden.file_of(&note);
    let outside = fs::read_to_string(&file)
        .expect("note file")
        .replace("Six plants.", "Eight plants.");

    let written_outside = outside.clone();
    before_the_next_exchange(move |file| fs::write(file, written_outside).expect("outside edit"));
    let answer = garden
        .board
        .apply(set_body(&note.id, "Six plants.", "Six plants and basil."));

    assert_eq!(
        fs::read_to_string(&file).expect("note file"),
        outside,
        "the edit replaced text it was not based on"
    );
    assert!(matches!(
        answer,
        Ok(Outcome::Conflict { current }) if current.body == "Eight plants."
    ));
}

#[test]
fn a_section_is_added_to_the_config_another_program_saved_between_the_read_and_the_write() {
    let garden = Garden::new("outside-config");
    let config = garden.folder().join(CONFIG_FILE);
    fs::write(&config, "{\n  \"version\": 1\n}\n").expect("config");

    before_the_next_exchange(|config| {
        fs::write(config, "{\n  \"version\": 1,\n  \"theme\": \"meadow\"\n}\n")
            .expect("outside save");
    });
    garden
        .board
        .apply(Op::AddSection {
            name: "Beds".to_owned(),
            parent: None,
            note: String::new(),
            hue: None,
        })
        .expect("section");

    let now = fs::read_to_string(&config).expect("config");
    assert!(
        now.contains("\"theme\": \"meadow\"") && now.contains("\"name\": \"Beds\""),
        "the config lost the key another program saved or the new section; it reads:\n{now}"
    );
}

#[test]
fn a_file_is_not_written_under_a_lock_for_something_else() {
    let garden = Garden::new("wrong-lock");
    let note = garden.add_note("Plant list", "Tomatoes");
    let config = garden.folder().join(CONFIG_FILE);
    fs::write(&config, "{}\n").expect("config");
    let held = BoardLocks::of_board(&garden.folder())
        .acquire(&[LockKey::Note(note.id.clone())], AT_ONCE)
        .expect("note lock");

    let replaced = replace_if_unchanged(&held, Target::Config, b"{}\n", b"{ }\n");
    assert!(
        replaced.is_err(),
        "the config was replaced under a note's lock: {replaced:?}"
    );
    let created = create_new(&held, Target::Config, b"{ }\n");
    assert!(
        created.is_err(),
        "the config was created under a note's lock: {created:?}"
    );
    assert_eq!(fs::read(&config).expect("config"), b"{}\n");
}

#[test]
fn a_new_file_never_replaces_one_that_is_there() {
    let garden = Garden::new("create-new");
    let note = garden.add_note("Plant list", "Tomatoes");
    let file = garden.file_of(&note);
    let name = file
        .file_name()
        .expect("name")
        .to_string_lossy()
        .into_owned();
    let before = fs::read(&file).expect("note file");
    let held = BoardLocks::of_board(&garden.folder())
        .acquire(&[LockKey::Note(note.id.clone())], AT_ONCE)
        .expect("note lock");
    let target = Target::Note {
        id: &note.id,
        name: &name,
    };

    assert_eq!(
        create_new(&held, target, b"other").expect("answer"),
        Created::Exists
    );
    assert_eq!(fs::read(&file).expect("note file"), before);
    assert_eq!(
        replace_if_unchanged(&held, target, &before, b"second, longer").expect("answer"),
        Replaced::Done
    );
    assert_eq!(fs::read(&file).expect("note file"), b"second, longer");
    assert_eq!(
        replace_if_unchanged(&held, target, &before, b"third").expect("answer"),
        Replaced::Changed {
            now: b"second, longer".to_vec()
        }
    );
    assert_eq!(fs::read(&file).expect("note file"), b"second, longer");
    assert_eq!(garden.left_in_the_locks_folder(), Vec::<String>::new());
}

fn die_as_the_writer_that_is_killed_before_its_exchange(root: &str) {
    let workspace = Workspace::open(Path::new(root)).expect("workspace");
    let board = workspace
        .board(&"garden".parse().expect("board id"))
        .expect("board handle");
    let note = NoteId::parse(&std::env::var(KILLED_WRITER_NOTE).expect("note")).expect("note id");
    before_the_next_exchange(|_| std::process::abort());
    let _ = board.apply(append(&note, "Written by the writer that was killed."));
}

#[test]
fn the_temporary_file_of_a_killed_writer_is_no_note_and_goes_with_the_next_write() {
    if let Ok(root) = std::env::var(KILLED_WRITER) {
        die_as_the_writer_that_is_killed_before_its_exchange(&root);
        return;
    }
    let garden = Garden::new("killed-writer");
    let note = garden.add_note("Harvest log", "Monday: peas.");
    let killed = Command::new(std::env::current_exe().expect("test binary"))
        .args([
            "outside_writers::the_temporary_file_of_a_killed_writer_is_no_note_and_goes_with_the_next_write",
            "--exact",
            "--nocapture",
        ])
        .env(KILLED_WRITER, &garden.root)
        .env(KILLED_WRITER_NOTE, note.id.as_str())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("writer process");
    assert!(!killed.success(), "the writer was not killed");

    let left = garden.left_in_the_locks_folder();
    assert_eq!(left.len(), 1, "the killed writer left {left:?}");
    assert!(
        fs::read_to_string(&left[0])
            .expect("temporary file")
            .contains("Written by the writer that was killed.")
    );
    let snapshot = garden.board.snapshot().expect("snapshot");
    assert_eq!(
        (snapshot.notes.len(), snapshot.strays),
        (1, Vec::<String>::new())
    );
    assert_eq!(snapshot.notes[0].body, "Monday: peas.");

    garden
        .board
        .apply(append(&note.id, "Tuesday: kale."))
        .expect("the next write");
    let left = garden.left_in_the_locks_folder();
    assert_eq!(
        left,
        Vec::<String>::new(),
        "the next writer of the note left the killed writer's temporary file behind"
    );
    assert_eq!(
        garden.board.note(&note.id).expect("note").body,
        "Monday: peas.\n\nTuesday: kale."
    );
}

#[test]
fn taking_a_lock_removes_what_a_killed_holder_of_that_lock_left() {
    let garden = Garden::new("leftovers");
    let note = garden.add_note("Plant list", "Tomatoes");
    let locks = garden.folder().join(LOCKS_FOLDER);
    let of_this_note = locks.join(format!("{}.tmp", note.id));
    let of_another_note = locks.join("n0ther1.tmp");
    let of_the_config = locks.join("board.config.tmp");
    for left in [&of_this_note, &of_another_note, &of_the_config] {
        fs::write(left, "half a file").expect("left file");
    }

    drop(
        BoardLocks::of_board(&garden.folder())
            .acquire(&[LockKey::Note(note.id.clone())], AT_ONCE)
            .expect("note lock"),
    );
    assert_eq!(
        garden.left_in_the_locks_folder(),
        [
            of_the_config.display().to_string(),
            of_another_note.display().to_string()
        ]
    );
    drop(
        BoardLocks::of_board(&garden.folder())
            .acquire(&[LockKey::Config], AT_ONCE)
            .expect("config lock"),
    );
    assert_eq!(
        garden.left_in_the_locks_folder(),
        [of_another_note.display().to_string()]
    );
}
