mod common;

use std::fs::{self, File};
use std::io::Write;
use std::thread;
use std::time::{Duration, Instant};

use common::{Scratch, add_note, add_section, with_an_editor_that_saves_in_time};
use wb_model::CONFIG_FILE;

const SAVE_TAKES: Duration = Duration::from_millis(40);
const SAVED_BY_AN_EDITOR: &str = "{\n  \"version\": 1,\n  \"theme\": \"meadow\",\n  \"sections\": [\n    {\n      \"id\": \"s000001\",\n      \"name\": \"Beds\"\n    }\n  ],\n  \"defaultSection\": \"s000001\"\n}\n";

fn start_a_save_in_place(config: &std::path::Path) -> thread::JoinHandle<Duration> {
    let mut emptied = File::create(config).expect("config emptied by the editor");
    let emptied_at = Instant::now();
    thread::spawn(move || {
        thread::sleep(SAVE_TAKES);
        emptied
            .write_all(SAVED_BY_AN_EDITOR.as_bytes())
            .expect("config written by the editor");
        emptied_at.elapsed()
    })
}

#[test]
fn a_section_added_while_an_editor_saves_the_config_in_place_wipes_nothing() {
    let now = with_an_editor_that_saves_in_time(|| {
        let scratch = Scratch::new("config-in-place");
        let board = scratch.garden();
        let config = scratch.path().join("garden").join(CONFIG_FILE);
        fs::write(&config, SAVED_BY_AN_EDITOR).expect("config");

        let editor = start_a_save_in_place(&config);
        board.apply(add_section("Paths")).expect("section");
        let took = editor.join().expect("editor");
        (took, fs::read_to_string(&config).expect("config"))
    });

    assert!(
        now.contains("\"theme\": \"meadow\"")
            && now.contains("\"name\": \"Beds\"")
            && now.contains("\"name\": \"Paths\""),
        "the config lost what the editor was saving or the new section; it reads:\n{now}"
    );
}

#[test]
fn a_note_added_while_an_editor_saves_the_config_in_place_lands_in_the_default_section() {
    let note = with_an_editor_that_saves_in_time(|| {
        let scratch = Scratch::new("config-in-place-note");
        let board = scratch.garden();
        let config = scratch.path().join("garden").join(CONFIG_FILE);
        fs::write(&config, SAVED_BY_AN_EDITOR).expect("config");

        let editor = start_a_save_in_place(&config);
        let note = add_note(&board, "Plant list", "Tomatoes");
        (editor.join().expect("editor"), note)
    });

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

const LINK_REFUSED: &str = " is a link or a folder, not a plain file, and the board writes only to plain files, so nothing was written to it or through it";

fn tick(path: &std::path::Path) -> String {
    format!("`{}`", path.display())
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

#[test]
fn a_note_in_a_file_named_for_another_id_is_found_by_the_id_inside_it() {
    let scratch = Scratch::new("name-and-id-differ");
    let board = scratch.garden();
    let folder = scratch.path().join("garden");
    let decoy = add_note(
        &board,
        "Lettuce",
        "A note whose file name ends in its own id.",
    );
    fs::write(
        folder.join("lettuce-n0ther1.md"),
        "---\nid: nreal01\n---\n\n# Lettuce\n\nSow in rows.\n",
    )
    .expect("note in a file named for another id");
    fs::write(folder.join("kale.md"), "---\nid: nreal02\n---\n\n# Kale\n").expect("plain name");
    let (lettuce, kale) = (
        wb_model::NoteId::parse("nreal01").expect("note id"),
        wb_model::NoteId::parse("nreal02").expect("note id"),
    );

    assert_eq!(board.note(&lettuce).expect("note").body, "Sow in rows.");
    board
        .apply(common::append(&lettuce, "Thin in May."))
        .expect("edit of a note in a file named for another id");
    assert_eq!(
        common::files_holding(&folder, &lettuce),
        ["lettuce-nreal01.md"]
    );
    board
        .apply(common::set_title(&kale, None, "Curly kale"))
        .expect("edit of a note in a file with a plain name");
    assert_eq!(
        common::files_holding(&folder, &kale),
        ["curly-kale-nreal02.md"]
    );
    assert_eq!(
        board.note(&decoy.id).expect("note").body,
        "A note whose file name ends in its own id."
    );
    assert!(matches!(
        board.note(&wb_model::NoteId::parse("n0ther1").expect("note id")),
        Err(wb_model::Error::NoteNotFound { .. })
    ));
}

#[test]
fn a_title_change_never_renames_over_a_file_that_holds_the_new_name() {
    let scratch = Scratch::new("title-collides");
    let board = scratch.garden();
    let folder = scratch.path().join("garden");
    let note = add_note(&board, "Carrots", "orange");
    let squatter = folder.join(format!("beets-{}.md", note.id));
    fs::write(&squatter, "hand written, no front matter\n").expect("hand-written file");

    let answer = board.apply(common::set_title(&note.id, Some("Carrots"), "Beets"));

    assert_eq!(
        fs::read_to_string(&squatter).expect("hand-written file"),
        "hand written, no front matter\n",
        "the title change renamed the note over a file that was there"
    );
    let kept = board.note_file(&note.id).expect("note");
    assert_eq!(
        (kept.name, kept.note.title, kept.note.body),
        (
            format!("carrots-{}.md", note.id),
            "Beets".to_owned(),
            "orange".to_owned()
        )
    );
    assert!(
        matches!(&answer, Ok(wb_model::Outcome::Applied { note }) if note.title == "Beets"),
        "{answer:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_note_that_is_a_link_is_listed_and_never_written_through_or_replaced() {
    let scratch = Scratch::new("linked-note");
    let board = scratch.garden();
    let folder = scratch.path().join("garden");
    let elsewhere = scratch.path().join("elsewhere.md");
    let linked_text = "---\nid: nlink01\n---\n\n# Linked\n\nkept outside the board\n";
    fs::write(&elsewhere, linked_text).expect("linked file");
    let link = folder.join("linked-nlink01.md");
    std::os::unix::fs::symlink(&elsewhere, &link).expect("link");
    let id = wb_model::NoteId::parse("nlink01").expect("note id");

    assert_eq!(
        board.note(&id).expect("note").body,
        "kept outside the board"
    );
    let answer = board.apply(common::append(&id, "added through the board"));

    assert_eq!(
        answer,
        Err(wb_model::Error::NotAPlainFile {
            path: link.display().to_string()
        })
    );
    assert_eq!(
        answer.expect_err("refused").to_string(),
        format!("{}{LINK_REFUSED}", tick(&link))
    );
    assert!(
        fs::symlink_metadata(&link)
            .expect("link")
            .file_type()
            .is_symlink(),
        "the link was turned into a plain file"
    );
    assert_eq!(
        fs::read_to_string(&elsewhere).expect("linked file"),
        linked_text
    );
    let left: Vec<_> = fs::read_dir(folder.join(".locks"))
        .expect("locks folder")
        .map(|entry| entry.expect("entry").file_name())
        .filter(|name| !name.to_string_lossy().ends_with(".lock"))
        .collect();
    assert_eq!(left, Vec::<std::ffi::OsString>::new());
}

#[test]
fn every_outside_writer_is_also_seen_on_a_volume_without_an_exchange() {
    common::every_test_passes_again_without_an_exchange();
}

#[test]
fn every_outside_writer_is_also_seen_on_a_volume_with_hard_links_only() {
    common::every_test_passes_again_on("hard-links-only");
}

#[cfg(unix)]
#[test]
fn a_config_that_is_a_link_is_never_replaced() {
    let scratch = Scratch::new("linked-config");
    let board = scratch.garden();
    let elsewhere = scratch.path().join("elsewhere.json");
    fs::write(&elsewhere, "{\n  \"version\": 1\n}\n").expect("linked config");
    let config = scratch.path().join("garden").join(CONFIG_FILE);
    std::os::unix::fs::symlink(&elsewhere, &config).expect("link");

    let answer = board.apply(add_section("Beds"));

    assert_eq!(
        answer.clone().map_err(|error| error.to_string()),
        Err(format!("{}{LINK_REFUSED}", tick(&config)))
    );
    assert_eq!(
        answer,
        Err(wb_model::Error::NotAPlainFile {
            path: config.display().to_string()
        })
    );
    assert!(
        fs::symlink_metadata(&config)
            .expect("link")
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_to_string(&elsewhere).expect("linked config"),
        "{\n  \"version\": 1\n}\n"
    );
}

#[cfg(unix)]
#[test]
fn a_config_that_is_a_link_to_nothing_is_refused_as_a_link_and_nothing_is_created() {
    let scratch = Scratch::new("dangling-config");
    let board = scratch.garden();
    let nowhere = scratch.path().join("nowhere.json");
    let config = scratch.path().join("garden").join(CONFIG_FILE);
    std::os::unix::fs::symlink(&nowhere, &config).expect("link");

    let answer = board.apply(add_section("Beds"));

    assert_eq!(
        answer.map_err(|error| error.to_string()),
        Err(format!("{}{LINK_REFUSED}", tick(&config)))
    );
    assert!(
        fs::symlink_metadata(&config)
            .expect("link")
            .file_type()
            .is_symlink()
    );
    assert!(!nowhere.exists(), "the config was created through the link");
}
