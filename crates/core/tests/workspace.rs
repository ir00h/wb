mod common;

use std::fs;

use common::{Scratch, add_note, add_section, tree_bytes};
use wb_core::Workspace;
use wb_model::{
    BoardId, Error, Invalid, NoteDraft, NoteId, Op, Outcome, SectionNode, SectionPath, SectionRef,
    Tag,
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
        "{\n  \"version\": 2,\n  \"name\": \"Kitchen garden\"\n}\n"
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
            })
            .expect("section"),
    );
    let draft = NoteDraft {
        title: "Plant list".to_owned(),
        body: "Tomatoes\n".to_owned(),
        section: Some(SectionRef::Path {
            path: "beds / raised beds"
                .parse::<String>()
                .expect("text")
                .try_into()
                .expect("path"),
        }),
        tags: vec!["Vegetable".to_owned(), "#open question".to_owned()],
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
            "---\nid: {}\nsection: Beds / Raised beds\ntags: [vegetable, open-question]\n---\n\n# Plant list\n\nTomatoes\n",
            note.id
        )
    );
    assert_eq!(board.note(&note.id).expect("note").title, "Plant list");

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
    assert_eq!(add_note(&board, "Plant list", "").section, Some(beds.id));
}

#[test]
fn a_section_that_does_not_exist_cannot_be_named() {
    let scratch = Scratch::new("no-section");
    let board = scratch.garden();
    let missing = SectionRef::Path {
        path: path("Cellar"),
    };
    let draft = NoteDraft {
        section: Some(missing.clone()),
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
