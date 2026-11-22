use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use wb_model::{
    BodyChange, CONFIG_FILE, Change, Error, NoteCard, NoteDraft, NoteEdit, NoteId, Op, Outcome,
};

use crate::lock::{BoardLocks, LOCKS_FOLDER, LockKey};
use crate::replace::hook::{Moment, at_the_next, before_the_next_exchange};
use crate::replace::{Created, Replaced, Target, create_new, replace_if_unchanged};
use crate::{Board, Replacement, TRASH_FOLDER, Volume, Workspace};

const VOLUME: &str = "WB_TEST_VOLUME";
const KILLED_WRITER: &str = "WB_TEST_KILLED_WRITER";
const KILLED_WRITER_NOTE: &str = "WB_TEST_KILLED_WRITER_NOTE";
static SCRATCH_FOLDERS: AtomicU32 = AtomicU32::new(0);
const AT_ONCE: Duration = Duration::from_millis(50);

fn volume_under_test() -> Option<Volume> {
    match std::env::var(VOLUME).as_deref() {
        Err(_) => None,
        Ok("without-exchange") => Some(Volume::WITHOUT_EXCHANGE),
        Ok("hard-links-only") => Some(Volume::WITH_HARD_LINKS_ONLY),
        Ok(other) => panic!("no volume called {other}"),
    }
}

fn locks_of(board: &Path) -> BoardLocks {
    BoardLocks::on_volume(board, volume_under_test())
}

fn when_the_core_can_still_see_it(action: impl FnOnce(&Path) + 'static) {
    let moment = match volume_under_test() {
        Some(volume) if volume.replacement == Replacement::RenameAfterALastLook => {
            Moment::BeforeTheLastLook
        }
        _ => Moment::AfterTheLastLook,
    };
    at_the_next(moment, action);
}

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
        let mut workspace = Workspace::open(&root).expect("workspace");
        if let Some(volume) = volume_under_test() {
            workspace = workspace.assuming_volume(volume);
        }
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

    when_the_core_can_still_see_it(|file| {
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
    when_the_core_can_still_see_it(move |file| {
        fs::write(file, written_outside).expect("outside edit");
    });
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
    when_the_core_can_still_see_it(move |file| {
        fs::write(file, written_outside).expect("outside edit");
    });
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

    when_the_core_can_still_see_it(|config| {
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
    let held = locks_of(&garden.folder())
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
    let held = locks_of(&garden.folder())
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
    let locks = locks_of(&garden.folder());
    drop(
        locks
            .acquire(&[LockKey::Config], AT_ONCE)
            .expect("config lock"),
    );
    let folder = garden.folder().join(LOCKS_FOLDER);
    let in_locks = |name: &str| folder.join(name).display().to_string();
    let of_this_note = [
        in_locks(&format!("{}.probe", note.id)),
        in_locks(&format!("{}.tmp", note.id)),
    ];
    let of_the_config = [in_locks("board.config.probe"), in_locks("board.config.tmp")];
    let of_another_note = [in_locks("n0ther1.probe"), in_locks("n0ther1.tmp")];
    for left in of_this_note
        .iter()
        .chain(&of_the_config)
        .chain(&of_another_note)
    {
        fs::write(left, "half a file").expect("left file");
    }

    drop(
        locks
            .acquire(&[LockKey::Note(note.id.clone())], AT_ONCE)
            .expect("note lock"),
    );
    assert_eq!(
        garden.left_in_the_locks_folder(),
        [of_the_config.clone(), of_another_note.clone()].concat(),
        "taking the lock of the note kept what a killed holder of it left"
    );
    drop(
        locks
            .acquire(&[LockKey::Config], AT_ONCE)
            .expect("config lock"),
    );
    assert_eq!(
        garden.left_in_the_locks_folder(),
        of_another_note.to_vec(),
        "taking the lock of the config kept what a killed holder of it left"
    );
}

#[test]
fn a_file_that_changed_before_the_exchange_is_never_exchanged() {
    let garden = Garden::new("changed-before");
    let note = garden.add_note("Plant list", "Tomatoes");
    let file = garden.file_of(&note);
    let name = file
        .file_name()
        .expect("name")
        .to_string_lossy()
        .into_owned();
    let held = locks_of(&garden.folder())
        .acquire(&[LockKey::Note(note.id.clone())], AT_ONCE)
        .expect("note lock");
    let target = Target::Note {
        id: &note.id,
        name: &name,
    };
    fs::write(&file, "written by another program").expect("outside edit");

    before_the_next_exchange(|_| panic!("the file was exchanged although it had changed"));
    let replaced = replace_if_unchanged(&held, target, b"read earlier", b"new").expect("answer");
    before_the_next_exchange(|_| {});

    assert_eq!(
        replaced,
        Replaced::Changed {
            now: b"written by another program".to_vec()
        }
    );
    assert_eq!(
        fs::read(&file).expect("note file"),
        b"written by another program"
    );
}

#[test]
fn an_outside_edit_of_the_same_length_is_a_conflict() {
    let garden = Garden::new("same-length");
    let note = garden.add_note("Tomato plan", "Six plants.");
    let file = garden.file_of(&note);
    let outside = fs::read_to_string(&file)
        .expect("note file")
        .replace("Six plants.", "Ten plants.");

    let written_outside = outside.clone();
    when_the_core_can_still_see_it(move |file| {
        fs::write(file, written_outside).expect("outside edit");
    });
    let answer = garden
        .board
        .apply(set_body(&note.id, "Six plants.", "Six plants and basil."));

    assert_eq!(
        fs::read_to_string(&file).expect("note file"),
        outside,
        "the edit replaced an outside text of the same length"
    );
    assert!(
        matches!(&answer, Ok(Outcome::Conflict { current }) if current.body == "Ten plants."),
        "{answer:?}"
    );
}

#[test]
fn an_append_keeps_a_paragraph_another_program_added_at_the_end_at_the_last_moment() {
    let garden = Garden::new("outside-adds-at-the-end");
    let note = garden.add_note("Harvest log", "Monday: peas.");
    let file = garden.file_of(&note);
    let outside = fs::read_to_string(&file).expect("note file") + "\nWednesday: leeks.\n";

    let written_outside = outside.clone();
    when_the_core_can_still_see_it(move |file| {
        fs::write(file, written_outside).expect("outside edit");
    });
    let answer = garden.board.apply(append(&note.id, "Tuesday: kale."));

    let now = fs::read_to_string(&file).expect("note file");
    assert_eq!(
        now,
        outside.clone() + "\nTuesday: kale.\n",
        "the paragraph another program added at the end is gone, answered {answer:?}"
    );
    assert!(
        matches!(&answer, Ok(Outcome::Applied { note }) if note.body == "Monday: peas.\n\nWednesday: leeks.\n\nTuesday: kale."),
        "{answer:?}"
    );
}

#[test]
fn an_append_keeps_an_outside_change_of_the_same_length_at_the_start() {
    let garden = Garden::new("same-length-at-the-start");
    let note = garden.add_note(
        "Tomato plan",
        "Six plants in the long bed by the south wall.",
    );
    let file = garden.file_of(&note);
    let outside = fs::read_to_string(&file)
        .expect("note file")
        .replace("# Tomato plan", "# Potato plan");

    let written_outside = outside.clone();
    when_the_core_can_still_see_it(move |file| {
        fs::write(file, written_outside).expect("outside edit");
    });
    let answer = garden.board.apply(append(&note.id, "And basil."));

    let now = fs::read_to_string(garden.file_of(&note)).expect("note file");
    assert!(
        now.contains(
            "# Potato plan\n\nSix plants in the long bed by the south wall.\n\nAnd basil.\n"
        ),
        "the title another program wrote is gone, answered {answer:?}; the file reads\n{now}"
    );
    assert!(
        matches!(&answer, Ok(Outcome::Applied { note }) if note.title == "Potato plan"),
        "{answer:?}"
    );
}

#[cfg(unix)]
#[test]
fn an_edit_that_cannot_land_answers_an_error_and_writes_nothing() {
    use std::os::unix::fs::PermissionsExt;

    let garden = Garden::new("cannot-land");
    let note = garden.add_note("Harvest log", "Monday: peas.");
    let file = garden.file_of(&note);
    let before = fs::read(&file).expect("note file");
    fs::set_permissions(garden.folder(), fs::Permissions::from_mode(0o555)).expect("read-only");

    let answer = garden.board.apply(append(&note.id, "Tuesday: kale."));

    fs::set_permissions(garden.folder(), fs::Permissions::from_mode(0o755)).expect("writable");
    assert_eq!(fs::read(&file).expect("note file"), before);
    assert!(
        matches!(answer, Err(Error::Io { .. })),
        "nothing was written, but the append answered {answer:?}"
    );
}

#[test]
fn a_config_emptied_at_the_last_moment_is_waited_out_and_never_taken_as_empty() {
    let garden = Garden::new("blank-at-the-last-look");
    let config = garden.folder().join(CONFIG_FILE);
    let saved = "{\n  \"version\": 1,\n  \"theme\": \"meadow\"\n}\n";
    fs::write(&config, saved).expect("config");
    let (hand_over, take_over) = std::sync::mpsc::channel::<fs::File>();
    let editor = std::thread::spawn(move || {
        let mut emptied = take_over.recv().expect("the editor's open file");
        std::thread::sleep(Duration::from_millis(40));
        std::io::Write::write_all(&mut emptied, saved.as_bytes()).expect("editor save");
    });

    when_the_core_can_still_see_it(move |config| {
        let emptied = fs::File::create(config).expect("the editor empties the config");
        hand_over.send(emptied).expect("hand over");
    });
    let answer = garden.board.apply(Op::AddSection {
        name: "Beds".to_owned(),
        parent: None,
        note: String::new(),
        hue: None,
    });
    editor.join().expect("editor");

    let now = fs::read_to_string(&config).expect("config");
    assert!(
        now.contains("\"theme\": \"meadow\"") && now.contains("\"name\": \"Beds\""),
        "the config the editor was saving was replaced by an empty one with the section:\n{now}"
    );
    assert!(
        matches!(answer, Ok(Outcome::SectionAdded { .. })),
        "{answer:?}"
    );
}

#[test]
fn a_config_holding_only_white_space_at_the_last_moment_is_waited_out_and_never_taken_as_empty() {
    let garden = Garden::new("white-space-at-the-last-look");
    let config = garden.folder().join(CONFIG_FILE);
    let saved = "{\n  \"version\": 1,\n  \"theme\": \"meadow\"\n}\n";
    fs::write(&config, saved).expect("config");
    let saving = config.clone();
    let (start, started) = std::sync::mpsc::channel::<()>();
    let editor = std::thread::spawn(move || {
        started.recv().expect("start");
        std::thread::sleep(Duration::from_millis(40));
        fs::write(saving, saved).expect("editor save");
    });

    when_the_core_can_still_see_it(move |config| {
        fs::write(config, "\n").expect("the editor leaves one empty line");
        start.send(()).expect("start");
    });
    let answer = garden.board.apply(Op::AddSection {
        name: "Beds".to_owned(),
        parent: None,
        note: String::new(),
        hue: None,
    });
    editor.join().expect("editor");

    let now = fs::read_to_string(&config).expect("config");
    assert!(
        now.contains("\"theme\": \"meadow\"") && now.contains("\"name\": \"Beds\""),
        "the config the editor was saving was replaced by an empty one with the section, answered {answer:?}:\n{now}"
    );
    assert!(
        matches!(answer, Ok(Outcome::SectionAdded { .. })),
        "{answer:?}"
    );
}

#[test]
fn a_config_another_program_creates_at_the_last_moment_keeps_its_keys_and_the_section() {
    let garden = Garden::new("created-at-the-last-moment");
    let config = garden.folder().join(CONFIG_FILE);
    assert!(!config.exists());

    at_the_next(Moment::BeforeTheCreation, |config| {
        fs::write(config, "{\n  \"version\": 1,\n  \"theme\": \"meadow\"\n}\n")
            .expect("outside save");
    });
    let answer = garden.board.apply(Op::AddSection {
        name: "Beds".to_owned(),
        parent: None,
        note: String::new(),
        hue: None,
    });

    let now = fs::read_to_string(&config).expect("config");
    assert!(
        now.contains("\"theme\": \"meadow\"") && now.contains("\"name\": \"Beds\""),
        "the section or the config another program created is gone, answered {answer:?}:\n{now}"
    );
}

#[test]
fn a_folder_put_in_place_of_a_note_at_the_last_moment_gets_nothing_and_blocks_nothing() {
    for moment in [Moment::BeforeTheLastLook, Moment::AfterTheLastLook] {
        let garden = Garden::new("folder-in-place");
        let note = garden.add_note("Weed the paths", "Before the rain.");
        let file = garden.file_of(&note);
        let before = fs::read(&file).expect("note file");
        let aside = garden.folder().join("aside.bin");

        let moved_aside = aside.clone();
        at_the_next(moment, move |file| {
            fs::rename(file, &moved_aside).expect("move aside");
            fs::create_dir(file).expect("folder in its place");
        });
        let answer = garden.board.apply(append(&note.id, "After the rain."));

        assert_eq!(
            answer,
            Err(Error::NotAPlainFile {
                path: file.display().to_string()
            }),
            "{moment:?}"
        );
        assert!(
            file.is_dir() && fs::read_dir(&file).expect("folder").next().is_none(),
            "{moment:?}: the folder put in place of the note was changed"
        );
        assert_eq!(fs::read(&aside).expect("moved-aside note"), before);
        assert_eq!(garden.left_in_the_locks_folder(), Vec::<String>::new());

        fs::remove_dir(&file).expect("folder removed");
        fs::rename(&aside, &file).expect("note put back");
        garden
            .board
            .apply(append(&note.id, "Later."))
            .expect("the next edit");
        assert_eq!(
            garden.board.note(&note.id).expect("note").body,
            "Before the rain.\n\nLater."
        );
    }
}

#[test]
fn a_new_board_never_takes_a_folder_whose_board_file_another_program_wrote_first() {
    let garden = Garden::new("board-file-written-first");
    let workspace = Workspace::open(&garden.root).expect("workspace");
    let theirs = "{\"name\":\"Kitchen\"}\n";

    at_the_next(Moment::BeforeTheCreation, move |board_file| {
        fs::write(board_file, theirs).expect("board file written by another program");
    });
    let added = workspace.add_board("Kitchen", None).expect("board");

    assert_eq!(added.id.as_str(), "kitchen-2");
    assert_eq!(
        fs::read_to_string(garden.root.join("kitchen/board.json")).expect("their board file"),
        theirs,
        "the new board was written over a board file another program wrote"
    );
    assert!(garden.root.join("kitchen-2/board.json").is_file());
}

#[test]
fn a_note_trashed_between_its_new_text_and_its_new_name_stays_in_the_trash() {
    let garden = Garden::new("trashed-before-the-rename");
    let note = garden.add_note("Tomato plan", "Six plants.");
    let file = garden.file_of(&note);

    at_the_next(Moment::BeforeTheRename, |file| {
        move_to_the_trash(file);
    });
    let answer = garden.board.apply(Op::EditNote {
        note: note.id.clone(),
        edit: NoteEdit {
            title: Some(Change {
                base: Some("Tomato plan".to_owned()),
                to: "Tomato plan 2027".to_owned(),
            }),
            body: None,
        },
    });

    assert_eq!(
        answer,
        Err(Error::NoteNotFound {
            note: note.id.clone()
        })
    );
    assert_eq!(garden.board.notes().expect("notes"), Vec::new());
    let trashed = garden
        .folder()
        .join(TRASH_FOLDER)
        .join(file.file_name().expect("file name"));
    assert!(
        fs::read_to_string(trashed)
            .expect("trashed note")
            .contains("# Tomato plan 2027")
    );
}

#[test]
fn every_test_of_outside_writers_passes_on_volumes_without_an_exchange() {
    if std::env::var_os(VOLUME).is_some() {
        return;
    }
    for volume in ["without-exchange", "hard-links-only"] {
        let run = Command::new(std::env::current_exe().expect("test binary"))
            .args(["outside_writers::", "--test-threads=1"])
            .env(VOLUME, volume)
            .output()
            .expect("test run");
        let said = String::from_utf8_lossy(&run.stdout);
        assert!(
            run.status.success(),
            "on a volume {volume} these tests failed:\n{}",
            said.lines()
                .filter(|line| line.contains("FAILED") || line.contains("panicked"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        assert!(
            said.contains(
                "test outside_writers::a_new_file_never_replaces_one_that_is_there ... ok"
            )
        );
    }
}
