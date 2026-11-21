mod common;

use std::fs::{self, File};
use std::io::Write;
use std::thread;
use std::time::Duration;

use common::{Scratch, add_note, add_section};
use wb_model::CONFIG_FILE;

const SAVE_TAKES: Duration = Duration::from_millis(40);
const SAVED_BY_AN_EDITOR: &str = "{\n  \"version\": 1,\n  \"theme\": \"meadow\",\n  \"sections\": [\n    {\n      \"id\": \"s000001\",\n      \"name\": \"Beds\"\n    }\n  ],\n  \"defaultSection\": \"s000001\"\n}\n";

fn start_a_save_in_place(config: &std::path::Path) -> thread::JoinHandle<()> {
    let mut emptied = File::create(config).expect("config emptied by the editor");
    thread::spawn(move || {
        thread::sleep(SAVE_TAKES);
        emptied
            .write_all(SAVED_BY_AN_EDITOR.as_bytes())
            .expect("config written by the editor");
    })
}

#[test]
fn a_section_added_while_an_editor_saves_the_config_in_place_wipes_nothing() {
    let scratch = Scratch::new("config-in-place");
    let board = scratch.garden();
    let config = scratch.path().join("garden").join(CONFIG_FILE);
    fs::write(&config, SAVED_BY_AN_EDITOR).expect("config");

    let editor = start_a_save_in_place(&config);
    board.apply(add_section("Paths")).expect("section");
    editor.join().expect("editor");

    let now = fs::read_to_string(&config).expect("config");
    assert!(
        now.contains("\"theme\": \"meadow\"")
            && now.contains("\"name\": \"Beds\"")
            && now.contains("\"name\": \"Paths\""),
        "the config lost what the editor was saving or the new section; it reads:\n{now}"
    );
}

#[test]
fn a_note_added_while_an_editor_saves_the_config_in_place_lands_in_the_default_section() {
    let scratch = Scratch::new("config-in-place-note");
    let board = scratch.garden();
    let config = scratch.path().join("garden").join(CONFIG_FILE);
    fs::write(&config, SAVED_BY_AN_EDITOR).expect("config");

    let editor = start_a_save_in_place(&config);
    let note = add_note(&board, "Plant list", "Tomatoes");
    editor.join().expect("editor");

    assert_eq!(
        note.section.as_ref().map(ToString::to_string),
        Some("s000001".to_owned()),
        "the note missed the default section of the config the editor was saving"
    );
}

#[test]
fn a_config_that_stays_blank_is_an_empty_config() {
    let scratch = Scratch::new("config-blank");
    let board = scratch.garden();
    let config = scratch.path().join("garden").join(CONFIG_FILE);
    fs::write(&config, "\n").expect("config");

    board.apply(add_section("Beds")).expect("section");

    let now = fs::read_to_string(&config).expect("config");
    assert!(now.starts_with("{\n  \"version\": 1,\n  \"sections\": [") && now.contains("Beds"));
}

#[cfg(unix)]
#[test]
fn an_edit_keeps_the_permission_bits_of_a_note_and_of_the_config() {
    use std::os::unix::fs::PermissionsExt;

    const OWNER_ONLY: u32 = 0o600;
    let mode =
        |path: &std::path::Path| fs::metadata(path).expect("file").permissions().mode() & 0o777;
    let scratch = Scratch::new("permissions");
    let board = scratch.garden();
    let folder = scratch.path().join("garden");
    let note = add_note(&board, "Diary", "Private.");
    let file = folder.join(common::files_holding(&folder, &note.id).remove(0));
    let config = folder.join(CONFIG_FILE);
    board.apply(add_section("Beds")).expect("section");
    for path in [&file, &config] {
        fs::set_permissions(path, fs::Permissions::from_mode(OWNER_ONLY)).expect("permissions");
    }

    board
        .apply(common::append(&note.id, "More."))
        .expect("edit");
    board.apply(add_section("Paths")).expect("section");

    assert_eq!(
        (mode(&file), mode(&config)),
        (OWNER_ONLY, OWNER_ONLY),
        "the note is now {:o} and the config {:o}, both were {OWNER_ONLY:o}",
        mode(&file),
        mode(&config)
    );
}

const FIRST_COPY: &str = "---\nid: ndup001\n---\n\n# Apple\n\nfirst copy\n";
const SECOND_COPY: &str = "---\nid: ndup001\n---\n\n# Apple\n\nsecond copy\n";

fn duplicated() -> wb_model::NoteId {
    wb_model::NoteId::parse("ndup001").expect("note id")
}

#[test]
fn an_edit_lands_in_the_file_the_board_shows_when_a_copy_holds_the_same_id() {
    let scratch = Scratch::new("copy-of-a-note");
    let board = scratch.garden();
    let folder = scratch.path().join("garden");
    fs::write(folder.join("apple-ndup001.md"), FIRST_COPY).expect("note");
    fs::write(folder.join("apple-ndup001 2.md"), SECOND_COPY).expect("copy made by a sync tool");
    let id = duplicated();

    let shown = board.snapshot().expect("snapshot");
    assert_eq!(shown.notes[0].body, "first copy");
    assert_eq!(shown.strays, ["apple-ndup001 2.md"]);

    board
        .apply(common::set_body(&id, Some("first copy"), "edited"))
        .expect("body edit");
    board
        .apply(common::set_title(&id, Some("Apple"), "Zucchini"))
        .expect("title edit");

    let shown = board.snapshot().expect("snapshot");
    assert_eq!(
        (shown.notes[0].title.as_str(), shown.notes[0].body.as_str()),
        ("Zucchini", "edited"),
        "the board shows a file the edit did not go to; strays: {:?}",
        shown.strays
    );
    assert_eq!(shown.strays, ["apple-ndup001 2.md"]);
    assert_eq!(board.note(&id).expect("note").body, "edited");
    assert_eq!(
        fs::read_to_string(folder.join("apple-ndup001 2.md")).expect("copy"),
        SECOND_COPY
    );
}

#[test]
fn an_edit_is_refused_when_two_files_named_for_one_id_hold_it() {
    let scratch = Scratch::new("two-files-one-id");
    let board = scratch.garden();
    let folder = scratch.path().join("garden");
    fs::write(folder.join("apple-ndup001.md"), FIRST_COPY).expect("note");
    fs::write(folder.join("pear-ndup001.md"), SECOND_COPY).expect("second file");
    let id = duplicated();

    let shown = board.snapshot().expect("snapshot");
    assert_eq!(shown.notes.len(), 1);
    assert_eq!(shown.notes[0].body, "first copy");
    assert_eq!(shown.strays, ["pear-ndup001.md"]);

    let answer = board.apply(common::append(&id, "edited"));
    assert_eq!(
        answer,
        Err(wb_model::Error::DuplicateNote {
            note: id.clone(),
            files: vec!["apple-ndup001.md".to_owned(), "pear-ndup001.md".to_owned()],
        }),
        "an edit of an id that two files hold was not refused"
    );
    assert_eq!(
        fs::read_to_string(folder.join("apple-ndup001.md")).expect("note"),
        FIRST_COPY
    );
    assert_eq!(
        fs::read_to_string(folder.join("pear-ndup001.md")).expect("second file"),
        SECOND_COPY
    );
    assert!(matches!(
        board.note(&id),
        Err(wb_model::Error::DuplicateNote { .. })
    ));
}

#[test]
fn an_edit_is_refused_when_two_files_named_for_nothing_hold_one_id() {
    let scratch = Scratch::new("two-plain-files-one-id");
    let board = scratch.garden();
    let folder = scratch.path().join("garden");
    fs::write(folder.join("apple.md"), FIRST_COPY).expect("note");
    fs::write(folder.join("pear.md"), SECOND_COPY).expect("second file");

    assert_eq!(
        board.apply(common::append(&duplicated(), "edited")),
        Err(wb_model::Error::DuplicateNote {
            note: duplicated(),
            files: vec!["apple.md".to_owned(), "pear.md".to_owned()],
        })
    );
}
