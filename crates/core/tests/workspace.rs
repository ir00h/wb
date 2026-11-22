mod common;

use std::fs;

use common::{Scratch, add_note, add_section, tree_bytes};
use wb_core::Workspace;
use wb_model::{
    BoardId, DraftSection, Error, Invalid, NoteDraft, NoteId, Op, Outcome, SectionNode,
    SectionPath, SectionRef, Tag,
};

fn board_id(text: &str) -> BoardId {
    BoardId::parse(text).expect("board id")
}

fn path(text: &str) -> SectionPath {
    SectionPath::parse(text).expect("section path")
}

fn added_section(outcome: Outcome) -> SectionNode {
    match outcome {
        Outcome::SectionAdded { section } => section,
        other => panic!("adding a section answered {other:?}"),
    }
}

#[test]
fn a_folder_that_does_not_exist_is_not_a_workspace() {
    let scratch = Scratch::new("missing");
    let missing = scratch.path().join("nowhere");
    assert!(matches!(
        Workspace::open(&missing),
        Err(Error::WorkspaceNotFound { .. })
    ));
}

#[test]
fn a_new_board_gets_a_folder_named_after_it_and_a_board_file() {
    let scratch = Scratch::new("add-board");
    let workspace = scratch.workspace();
    let garden = workspace
        .add_board("  Kitchen   garden ", None)
        .expect("board");
    assert_eq!(garden.id, board_id("kitchen-garden"));
    assert_eq!(garden.name, "Kitchen garden");
    assert_eq!(
        fs::read_to_string(scratch.path().join("kitchen-garden/board.json")).expect("board file"),
        "{\n  \"version\": 2,\n  \"name\": \"Kitchen garden\",\n  \"cards\": {},\n  \"sections\": {}\n}\n"
    );
    assert!(wb_core::is_board_folder(
        &scratch.path().join("kitchen-garden")
    ));

    let second = workspace.add_board("Kitchen garden", None).expect("board");
    assert_eq!(second.id, board_id("kitchen-garden-2"));
    let reserved = workspace.add_board("Files", None).expect("board");
    assert_eq!(reserved.id, board_id("board"));
    assert_eq!(
        workspace.add_board(" \t", None),
        Err(Error::Invalid {
            problem: Invalid::EmptyBoardName
        })
    );
}

#[test]
fn boards_are_listed_parents_first_and_siblings_by_folder_name() {
    let scratch = Scratch::new("list-boards");
    let workspace = scratch.workspace();
    let kitchen = workspace.add_board("Kitchen", None).expect("board");
    let garden = workspace.add_board("Garden", None).expect("board");
    workspace
        .add_board("Seeds", Some(&garden.id))
        .expect("board");
    workspace
        .add_board("Recipes", Some(&kitchen.id))
        .expect("board");
    fs::create_dir_all(scratch.path().join("garden/assets")).expect("assets");
    fs::write(scratch.path().join("garden/assets/board.json"), "{}").expect("stray board file");
    let plain = scratch.path().join("notes-without-a-board");
    fs::create_dir_all(&plain).expect("plain folder");
    assert!(!wb_core::is_board_folder(&plain));

    let listed: Vec<(String, String)> = workspace
        .boards()
        .expect("boards")
        .into_iter()
        .map(|board| (board.id.to_string(), board.name))
        .collect();
    assert_eq!(
        listed,
        [
            ("garden".to_owned(), "Garden".to_owned()),
            ("garden/seeds".to_owned(), "Seeds".to_owned()),
            ("kitchen".to_owned(), "Kitchen".to_owned()),
            ("kitchen/recipes".to_owned(), "Recipes".to_owned()),
        ]
    );
    assert!(matches!(
        workspace.board(&board_id("cellar")),
        Err(Error::BoardNotFound { .. })
    ));
    assert!(matches!(
        workspace.add_board("Jars", Some(&board_id("cellar"))),
        Err(Error::BoardNotFound { .. })
    ));
}

#[test]
fn a_new_note_lands_in_its_section_under_a_name_from_its_title() {
    let scratch = Scratch::new("add-note");
    let board = scratch.garden();
    let beds = added_section(board.apply(add_section("Beds")).expect("section"));
    let raised = added_section(
        board
            .apply(Op::AddSection {
                name: "Raised beds".to_owned(),
                parent: Some(SectionRef::Id {
                    id: beds.id.clone(),
                }),
                note: " Two by one metre. ".to_owned(),
                hue: Some(120.0),
            })
            .expect("section"),
    );
    assert_eq!(raised.hue, Some(120.0));
    let config =
        fs::read_to_string(scratch.path().join("garden/board.config.json")).expect("config");
    assert!(
        config.contains(&format!(
            "      \"parent\": \"{}\",\n      \"note\": \"Two by one metre.\",\n      \"hue\": 120\n",
            beds.id
        )),
        "the section's note and hue are not in the config:\n{config}"
    );
    assert_eq!(
        board.apply(Op::AddSection {
            name: "Paths".to_owned(),
            parent: None,
            note: String::new(),
            hue: Some(400.0),
        }),
        Err(Error::Invalid {
            problem: Invalid::HueOutOfRange
        })
    );
    let draft = NoteDraft {
        title: "Plant list".to_owned(),
        body: "Tomatoes\n".to_owned(),
        section: DraftSection::Named {
            section: SectionRef::Path {
                path: path("beds / raised beds"),
            },
        },
        tags: vec!["Vegetable".to_owned(), "#open question".to_owned()],
        source: " seed catalogue ".to_owned(),
    };
    let Outcome::Applied { note } = board.apply(Op::AddNote { draft }).expect("note") else {
        panic!("the note was not added");
    };
    assert_eq!(note.section, Some(raised.id));
    assert_eq!(note.tags, Tag::list(&["vegetable", "open-question"]));
    assert_eq!(
        fs::read_to_string(
            scratch
                .path()
                .join(format!("garden/plant-list-{}.md", note.id))
        )
        .expect("note file"),
        format!(
            "---\nid: {}\nsection: Beds / Raised beds\ntags: [vegetable, open-question]\nsource: seed catalogue\n---\n\n# Plant list\n\nTomatoes\n",
            note.id
        )
    );
    assert_eq!(board.note(&note.id).expect("note").title, "Plant list");
    let file = board.note_file(&note.id).expect("note file");
    assert_eq!(file.name, format!("plant-list-{}.md", note.id));
    assert_eq!(file.path, board.folder().join(&file.name));
    assert_eq!(file.path, scratch.path().join("garden").join(&file.name));
    assert_eq!(file.note.source, "seed catalogue");
    assert_eq!(board.notes().expect("notes"), [file]);

    let snapshot = board.snapshot().expect("snapshot");
    assert_eq!(snapshot.board.name, "Garden");
    assert_eq!(snapshot.notes, [note]);
    assert_eq!(snapshot.sections.len(), 1);
    assert_eq!(snapshot.sections[0].children[0].name, "Raised beds");
}

#[test]
fn a_note_that_names_no_section_goes_to_the_default_section() {
    let scratch = Scratch::new("default-section");
    let board = scratch.garden();
    let beds = added_section(board.apply(add_section("Beds")).expect("section"));
    let config = scratch.path().join("garden/board.config.json");
    let text = fs::read_to_string(&config).expect("config");
    let with_default = text.replacen(
        "\n}",
        &format!(",\n  \"defaultSection\": \"{}\"\n}}", beds.id),
        1,
    );
    fs::write(&config, with_default).expect("config");
    assert_eq!(
        add_note(&board, "Plant list", "").section,
        Some(beds.id.clone())
    );
    let outside_every_section = NoteDraft {
        title: "Loose thought".to_owned(),
        section: DraftSection::NoSection,
        ..NoteDraft::default()
    };
    let Outcome::Applied { note } = board
        .apply(Op::AddNote {
            draft: outside_every_section,
        })
        .expect("note")
    else {
        panic!("the note was not added");
    };
    assert_eq!(
        note.section, None,
        "a note that asked for no section was put into the default section"
    );
    assert!(
        !fs::read_to_string(
            scratch
                .path()
                .join(format!("garden/loose-thought-{}.md", note.id))
        )
        .expect("note file")
        .contains("section:")
    );
}

#[test]
fn a_section_that_does_not_exist_cannot_be_named() {
    let scratch = Scratch::new("no-section");
    let board = scratch.garden();
    let missing = SectionRef::Path {
        path: path("Cellar"),
    };
    let draft = NoteDraft {
        section: DraftSection::Named {
            section: missing.clone(),
        },
        ..NoteDraft::default()
    };
    assert_eq!(
        board.apply(Op::AddNote { draft }),
        Err(Error::SectionNotFound {
            section: missing.clone()
        })
    );
    assert_eq!(
        board.apply(Op::AddSection {
            name: "Shelves".to_owned(),
            parent: Some(missing.clone()),
            note: String::new(),
            hue: None,
        }),
        Err(Error::SectionNotFound { section: missing })
    );
    board.apply(add_section("Beds")).expect("section");
    assert_eq!(
        board.apply(add_section("beds")),
        Err(Error::Invalid {
            problem: Invalid::SectionNameTaken {
                name: "beds".to_owned()
            }
        })
    );
}

#[test]
fn an_edit_that_changes_nothing_leaves_a_hand_written_file_alone() {
    let scratch = Scratch::new("no-change");
    let board = scratch.garden();
    let by_hand = "---\r\ntags: herb\r\nid: n1\r\n---\r\n# Basil\r\nSow thinly.\r\n";
    let path = scratch.path().join("garden/basil.md");
    fs::write(&path, by_hand).expect("note file");
    let id = NoteId::parse("n1").expect("id");
    let outcome = board
        .apply(common::set_title(&id, Some("Basil"), "Basil"))
        .expect("edit");
    assert!(matches!(outcome, Outcome::Applied { note } if note.body == "Sow thinly."));
    assert_eq!(fs::read_to_string(&path).expect("note file"), by_hand);
}

#[test]
fn reading_a_board_writes_nothing_and_reports_files_without_an_id() {
    let scratch = Scratch::new("read-only");
    let board = scratch.garden();
    let note = add_note(&board, "Plant list", "Tomatoes");
    let folder = scratch.path().join("garden");
    fs::remove_dir_all(folder.join(".locks")).expect("locks folder");
    fs::write(folder.join("rhubarb.md"), "# Rhubarb\n\nNo id here.\n").expect("stray");
    fs::write(
        folder.join("zz-copy.md"),
        format!(
            "---\nid: {}\n---\n\n# A second file with the same id\n",
            note.id
        ),
    )
    .expect("duplicate");
    fs::write(folder.join(".hidden.md"), "---\nid: n9\n---\n").expect("hidden");

    let before = tree_bytes(scratch.path());
    let snapshot = board.snapshot().expect("snapshot");
    assert_eq!(snapshot.notes, std::slice::from_ref(&note));
    assert_eq!(snapshot.strays, ["rhubarb.md", "zz-copy.md"]);
    assert_eq!(board.note(&note.id).expect("note").body, "Tomatoes");
    assert_eq!(tree_bytes(scratch.path()), before);
}

#[test]
fn a_config_that_is_not_valid_is_reported_and_never_overwritten() {
    let scratch = Scratch::new("bad-config");
    let board = scratch.garden();
    let config = scratch.path().join("garden/board.config.json");
    fs::write(&config, "{ \"sections\": [").expect("config");
    assert!(matches!(board.snapshot(), Err(Error::Malformed { .. })));
    assert!(matches!(
        board.apply(add_section("Beds")),
        Err(Error::Malformed { .. })
    ));
    assert_eq!(
        fs::read_to_string(&config).expect("config"),
        "{ \"sections\": ["
    );
}

#[test]
fn a_note_with_a_tag_that_is_not_well_formed_is_refused_and_nothing_is_written() {
    let scratch = Scratch::new("malformed-tag");
    let board = scratch.garden();
    let before = tree_bytes(scratch.path());
    let draft = NoteDraft {
        title: "Plant list".to_owned(),
        tags: vec!["vegetable".to_owned(), "Plan!".to_owned()],
        ..NoteDraft::default()
    };
    assert_eq!(
        board.apply(Op::AddNote { draft }),
        Err(Error::Invalid {
            problem: Invalid::MalformedTag {
                tag: "plan!".to_owned()
            }
        })
    );
    assert_eq!(tree_bytes(scratch.path()), before);
}

#[test]
fn the_notes_of_a_board_are_listed_whole_with_their_files_in_the_order_of_a_snapshot() {
    let scratch = Scratch::new("notes-with-files");
    let board = scratch.garden();
    let seeds = add_note(&board, "Seeds", "Beans");
    let plants = add_note(&board, "Plant list", "Tomatoes");
    let folder = scratch.path().join("garden");
    fs::write(folder.join("rhubarb.md"), "# Rhubarb\n").expect("stray");
    fs::write(
        folder.join("zz-copy.md"),
        format!("---\nid: {}\n---\n\n# A copy\n", seeds.id),
    )
    .expect("copy");

    let before = tree_bytes(scratch.path());
    let listed = board.notes().expect("notes");
    let snapshot = board.snapshot().expect("snapshot");
    assert_eq!(tree_bytes(scratch.path()), before);

    let ids = |notes: Vec<NoteId>| notes;
    assert_eq!(
        ids(listed.iter().map(|file| file.note.id.clone()).collect()),
        ids(snapshot.notes.iter().map(|note| note.id.clone()).collect())
    );
    for file in &listed {
        assert_eq!(file.path, folder.join(&file.name));
        assert_eq!(
            wb_model::parse(&fs::read_to_string(&file.path).expect("note file")),
            wb_model::Parsed::Note(Box::new(file.note.clone()))
        );
    }
    let names: Vec<&str> = listed.iter().map(|file| file.name.as_str()).collect();
    assert_eq!(
        names,
        [
            format!("plant-list-{}.md", plants.id),
            format!("seeds-{}.md", seeds.id)
        ]
    );
}

#[test]
fn an_edit_of_an_id_no_file_holds_leaves_nothing_behind() {
    let scratch = Scratch::new("unknown-id");
    let board = scratch.garden();
    let before = tree_bytes(scratch.path());
    let unknown = NoteId::parse("n99").expect("id");
    assert_eq!(
        board.apply(common::append(&unknown, "Nobody reads this.")),
        Err(Error::NoteNotFound {
            note: unknown.clone()
        })
    );
    let after = tree_bytes(scratch.path());
    assert_eq!(
        after.keys().collect::<Vec<_>>(),
        before.keys().collect::<Vec<_>>(),
        "the edit of an unknown id left files behind"
    );
    assert!(!scratch.path().join("garden/.locks/n99.lock").exists());
}

#[test]
fn a_board_whose_creation_was_cut_short_is_no_board_and_its_name_is_free_again() {
    let scratch = Scratch::new("cut-short");
    let locks = scratch.path().join("garden/.locks");
    fs::create_dir_all(&locks).expect("locks folder");
    fs::write(locks.join("board.config.lock"), "").expect("lock file");
    fs::write(
        locks.join("board.config.tmp"),
        "{\n  \"version\": 2,\n  \"na",
    )
    .expect("half a board file");
    let workspace = scratch.workspace();

    assert_eq!(workspace.boards().expect("boards"), Vec::new());
    assert_eq!(
        workspace
            .board(&board_id("garden"))
            .map(|board| board.id().clone()),
        Err(Error::BoardNotFound {
            board: board_id("garden")
        })
    );
    let garden = workspace.add_board("Garden", None).expect("board");

    assert_eq!(garden.id, board_id("garden"));
    assert_eq!(
        workspace
            .board(&garden.id)
            .expect("board")
            .snapshot()
            .expect("snapshot")
            .board
            .name,
        "Garden"
    );
    let left: Vec<_> = fs::read_dir(&locks)
        .expect("locks folder")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(left, ["board.config.lock"]);
}

#[test]
fn a_board_gives_its_name_and_config_as_a_snapshot_reads_them_and_writes_nothing() {
    let scratch = Scratch::new("name-and-config");
    let board = scratch.garden();
    let config = scratch.path().join("garden").join(wb_model::CONFIG_FILE);

    assert_eq!(board.info().expect("info").name, "Garden");
    assert_eq!(
        board.config().expect("config"),
        wb_model::BoardConfig::default()
    );
    fs::write(&config, " \n").expect("blank config");
    assert_eq!(
        board.config().expect("config"),
        wb_model::BoardConfig::default()
    );

    board.apply(add_section("Beds")).expect("section");
    fs::write(
        scratch.path().join("garden/board.json"),
        "{\"version\":2,\"name\":\"Kitchen garden\"}\n",
    )
    .expect("board file renamed by another program");
    let before = tree_bytes(scratch.path());

    let info = board.info().expect("info");
    let read = board.config().expect("config");
    assert_eq!(
        (info.id.as_str(), info.name.as_str()),
        ("garden", "Kitchen garden")
    );
    assert_eq!(read.section_entries()[0].name, "Beds");
    let snapshot = board.snapshot().expect("snapshot");
    assert_eq!(snapshot.board, info);
    assert_eq!(
        snapshot.sections,
        wb_model::SectionTree::from_config(&read).nodes()
    );
    assert_eq!(
        tree_bytes(scratch.path()),
        before,
        "reading the name or the config wrote a file"
    );
}

#[test]
fn a_new_board_never_takes_a_folder_of_its_name_that_holds_other_files() {
    let scratch = Scratch::new("folder-of-its-name");
    let theirs = scratch.path().join("garden");
    fs::create_dir_all(&theirs).expect("their folder");
    fs::write(theirs.join("plant-list.txt"), "Tomatoes\n").expect("their file");

    let garden = scratch
        .workspace()
        .add_board("Garden", None)
        .expect("board");

    assert_eq!(garden.id, board_id("garden-2"));
    assert!(!theirs.join("board.json").exists());
    assert!(!theirs.join(".locks").exists());
}
