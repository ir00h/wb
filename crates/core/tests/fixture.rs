mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{Scratch, append, copy_tree, open_board, tree_bytes};
use serde_json::{Value, json};
use wb_model::{BoardConfig, NoteCard, NoteId, Op, Outcome, Parsed, Tag, parse, render};

const GARDEN: &str = "garden";
const SEEDS: &str = "garden/seeds";
const TOMATO_FILE: &str = "garden/tomato-plan-for-the-south-fence-npp7oi4.md";
const TOMATO_FRONT_MATTER: &str = "---\nid: npp7oi4\nsection: Beds / Raised beds\ntags: [vegetable, chore]\nsource: seed catalogue\nvariety: San Marzano\n---\n";
const WRITTEN_NOTE_FOLDERS: [&str; 3] = ["garden", "garden/.trash", "garden/seeds"];
const CONFIG_FILES: [&str; 3] = [
    "workspace/garden/board.config.json",
    "workspace/garden/seeds/board.config.json",
    "odd/board.config.json",
];

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn workspace_copy(name: &str) -> Scratch {
    let scratch = Scratch::new(name);
    copy_tree(&fixtures().join("workspace"), scratch.path());
    scratch
}

fn drop_into_garden(scratch: &Scratch, odd_file: &str, as_name: &str) {
    fs::copy(
        fixtures().join("odd").join(odd_file),
        scratch.path().join(GARDEN).join(as_name),
    )
    .expect("odd file");
}

fn card(
    id: &str,
    title: &str,
    tags: &[&str],
    section: Option<&str>,
) -> (String, String, Vec<Tag>, Option<String>) {
    (
        id.to_owned(),
        title.to_owned(),
        Tag::list(tags),
        section.map(str::to_owned),
    )
}

fn listed(notes: &[NoteCard]) -> Vec<(String, String, Vec<Tag>, Option<String>)> {
    notes
        .iter()
        .map(|note| {
            (
                note.id.to_string(),
                note.title.clone(),
                note.tags.clone(),
                note.section.as_ref().map(ToString::to_string),
            )
        })
        .collect()
}

fn changed_files(
    scratch: &Scratch,
    before: &std::collections::BTreeMap<String, Vec<u8>>,
) -> Vec<String> {
    let after = tree_bytes(scratch.path());
    let mut changed: Vec<String> = after
        .iter()
        .filter(|(path, bytes)| before.get(*path) != Some(*bytes))
        .map(|(path, _)| path.clone())
        .chain(
            before
                .keys()
                .filter(|path| !after.contains_key(*path))
                .cloned(),
        )
        .filter(|path| !path.contains("/.locks/"))
        .collect();
    changed.sort();
    changed
}

#[test]
fn a_board_in_the_existing_format_lists_every_note_with_its_section() {
    let scratch = workspace_copy("fixture-snapshot");
    drop_into_garden(&scratch, "rhubarb-crumble.md", "rhubarb-crumble.md");

    let garden = open_board(scratch.path(), GARDEN)
        .snapshot()
        .expect("snapshot");
    assert_eq!(garden.board.name, "Garden");
    assert_eq!(
        listed(&garden.notes),
        [
            card("n7sv7us", "Basil row", &["herb"], Some("scrizb6")),
            card(
                "nn9fbyt",
                "Pick beans",
                &["vegetable", "chore"],
                Some("scrizb6")
            ),
            card(
                "ncffqm3",
                "Pick peas",
                &["vegetable", "chore"],
                Some("scrizb6")
            ),
            card("nmrr8rt", "Rules for this board", &[], None),
            card("ngd1ddx", "Seed swap list", &["done"], Some("szevnsg")),
            card("n7adqwg", "Seeds", &[], Some("scrizb6")),
            card(
                "npp7oi4",
                "Tomato plan for the south fence",
                &["vegetable", "chore"],
                Some("s6dj313")
            ),
            card("nivu23r", "Turn the heap", &["chore"], Some("s1tytqd")),
        ]
    );
    assert_eq!(garden.strays, ["rhubarb-crumble.md"]);
    let top: Vec<&str> = garden
        .sections
        .iter()
        .map(|section| section.name.as_str())
        .collect();
    assert_eq!(top, ["Beds", "Harvest", "Compost"]);
    assert_eq!(garden.sections[0].children[0].name, "Raised beds");
    assert_eq!(garden.sections[0].hue, Some(120.0));

    let seeds = open_board(scratch.path(), SEEDS)
        .snapshot()
        .expect("snapshot");
    assert_eq!(seeds.board.name, "Seeds");
    assert_eq!(
        listed(&seeds.notes),
        [card(
            "n2rv8u7",
            "Bean packet",
            &["vegetable"],
            Some("s0kgwmj")
        )]
    );
    assert_eq!(seeds.strays, Vec::<String>::new());
}

#[test]
fn every_written_note_file_renders_back_byte_for_byte() {
    let workspace = fixtures().join("workspace");
    let mut checked = 0;
    let mut different = Vec::new();
    for folder in WRITTEN_NOTE_FOLDERS {
        for entry in fs::read_dir(workspace.join(folder)).expect("folder") {
            let path = entry.expect("entry").path();
            if path.extension().is_none_or(|extension| extension != "md") {
                continue;
            }
            let text = fs::read_to_string(&path).expect("note file");
            let rendered = match parse(&text) {
                Parsed::Note(note) => render(&note),
                Parsed::Stray { .. } => "a stray".to_owned(),
            };
            checked += 1;
            if rendered != text {
                different.push(format!("{}:\n{rendered}", path.display()));
            }
        }
    }
    assert_eq!(checked, 11);
    assert!(different.is_empty(), "{}", different.join("\n"));
}

#[test]
fn every_config_file_renders_back_byte_for_byte() {
    for file in CONFIG_FILES {
        let text = fs::read_to_string(fixtures().join(file)).expect("config file");
        let config = BoardConfig::parse(&text).expect("config");
        assert_eq!(config.render().expect("text"), text, "{file}");
    }
}

#[test]
fn taking_a_snapshot_changes_no_byte_in_the_workspace() {
    let scratch = workspace_copy("fixture-read-only");
    drop_into_garden(&scratch, "rhubarb-crumble.md", "rhubarb-crumble.md");
    drop_into_garden(&scratch, "plum-jam-n0dd002.md", "plum-jam-n0dd002.md");
    drop_into_garden(
        &scratch,
        "companion-planting-n0dd001.md",
        "companion-planting-n0dd001.md",
    );
    let before = tree_bytes(scratch.path());

    let boards = scratch.workspace().boards().expect("boards");
    assert_eq!(boards.len(), 2);
    open_board(scratch.path(), GARDEN)
        .snapshot()
        .expect("snapshot");
    open_board(scratch.path(), SEEDS)
        .snapshot()
        .expect("snapshot");
    assert_eq!(tree_bytes(scratch.path()), before);
}

#[test]
fn one_edit_changes_one_note_file_and_keeps_its_unknown_key_in_place() {
    let scratch = workspace_copy("fixture-edit");
    let before = tree_bytes(scratch.path());
    assert!(before[TOMATO_FILE].starts_with(TOMATO_FRONT_MATTER.as_bytes()));

    let tomato = NoteId::parse("npp7oi4").expect("note id");
    let outcome = open_board(scratch.path(), GARDEN)
        .apply(append(&tomato, "Pinch out the side shoots."))
        .expect("edit");
    assert!(matches!(outcome, Outcome::Applied { .. }));

    assert_eq!(changed_files(&scratch, &before), [TOMATO_FILE]);
    let after = fs::read_to_string(scratch.path().join(TOMATO_FILE)).expect("note file");
    assert!(after.starts_with(TOMATO_FRONT_MATTER), "{after}");
    assert!(
        after.ends_with("\n\nPinch out the side shoots.\n"),
        "{after}"
    );
}

#[test]
fn adding_a_section_keeps_unknown_keys_at_every_level_of_the_config() {
    let scratch = workspace_copy("fixture-config");
    let config_path = scratch.path().join("garden/board.config.json");
    fs::copy(fixtures().join("odd/board.config.json"), &config_path).expect("odd config");
    let before = tree_bytes(scratch.path());
    let read = |path: &Path| -> Value {
        serde_json::from_str(&fs::read_to_string(path).expect("config")).expect("json")
    };
    let mut expected = read(&config_path);

    let outcome = open_board(scratch.path(), GARDEN)
        .apply(Op::AddSection {
            name: "Orchard".to_owned(),
            parent: None,
        })
        .expect("section");
    let Outcome::SectionAdded { section } = outcome else {
        panic!("no section was added");
    };

    let written = read(&config_path);
    assert_eq!(written["theme"], json!({ "paper": "kraft" }));
    assert_eq!(written["sections"][0]["pinned"], json!(true));
    assert_eq!(written["tags"][0]["icon"], json!("leaf"));
    expected["sections"]
        .as_array_mut()
        .expect("sections")
        .push(json!({ "id": section.id, "name": "Orchard" }));
    assert_eq!(written, expected);
    assert_eq!(
        changed_files(&scratch, &before),
        ["garden/board.config.json"]
    );
}

#[test]
fn a_note_with_windows_line_endings_and_a_byte_order_mark_loads() {
    let scratch = workspace_copy("fixture-crlf");
    drop_into_garden(&scratch, "plum-jam-n0dd002.md", "plum-jam-n0dd002.md");
    let written = fs::read(scratch.path().join("garden/plum-jam-n0dd002.md")).expect("note file");
    assert!(written.starts_with(b"\xef\xbb\xbf---\r\n"));

    let board = open_board(scratch.path(), GARDEN);
    let plum = NoteId::parse("n0dd002").expect("note id");
    let note = board.note(&plum).expect("note");
    assert_eq!(note.title, "Plum jam");
    assert_eq!(note.body, "One kilo of plums.\nHalf a kilo of sugar.");
    assert_eq!(note.tags, Tag::list(&["plum", "jam"]));
    let snapshot = board.snapshot().expect("snapshot");
    let card = snapshot
        .notes
        .iter()
        .find(|card| card.id == plum)
        .expect("card");
    assert_eq!(
        card.section.as_ref().map(ToString::to_string).as_deref(),
        Some("scrizb6")
    );
}

#[test]
fn an_edit_keeps_a_kind_the_format_does_not_know_and_repeated_keys() {
    let scratch = workspace_copy("fixture-kind");
    drop_into_garden(
        &scratch,
        "companion-planting-n0dd001.md",
        "companion-planting-n0dd001.md",
    );
    let companion = NoteId::parse("n0dd001").expect("note id");
    open_board(scratch.path(), GARDEN)
        .apply(append(&companion, "Nasturtiums by the squash."))
        .expect("edit");
    assert_eq!(
        fs::read_to_string(scratch.path().join("garden/companion-planting-n0dd001.md"))
            .expect("note file"),
        "---\nid: n0dd001\nkind: idea\nsection: Beds\ntags: [herb, open-question]\nseason: spring\naliases:\n  - companions\n  - planting pairs\nrating: \"4/5\"\nseason: summer\nempty:\n---\n\n# Companion planting\n\nMarigolds next to the beans.\n\nNasturtiums by the squash.\n"
    );
}
