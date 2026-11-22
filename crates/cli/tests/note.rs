mod common;

use std::fs;

use common::{Garden, text_of};
use serde_json::{Value, json};

const PLANT: &str = "nPLANT0";
const WATER: &str = "nWATER0";
const EMPTY: &str = "nEMPTY0";

fn added_json(id: &str, file: &str, section: &Value, tags: &[&str]) -> String {
    serde_json::to_string_pretty(&json!({
        "id": id,
        "path": format!("<tmp>/ws/garden/{file}"),
        "board": "garden",
        "section": section,
        "stack": null,
        "order": null,
        "tags": tags,
    }))
    .expect("JSON")
        + "\n"
}

#[test]
fn note_add_prints_the_id_and_file() {
    let garden = Garden::new("note-add");
    let run = garden
        .wb(&[
            "note",
            "add",
            "--title",
            "Plant list",
            "--section",
            "Beds",
            "--tag",
            "plan",
            "--body",
            "Tomatoes, basil and beans.",
        ])
        .ok();
    let id = run.stdout.lines().next().expect("an id").to_owned();
    assert_eq!(
        garden.normalise(&run.stdout, &[(PLANT, &id)]),
        "nPLANT0\n<tmp>/ws/garden/plant-list-nPLANT0.md\n"
    );
    assert_eq!(
        garden.scratch.read(&format!("garden/plant-list-{id}.md")),
        format!(
            "---\nid: {id}\nsection: Beds\ntags: [plan]\n---\n\n# Plant list\n\nTomatoes, basil and beans.\n"
        )
    );
}

#[test]
fn note_add_prints_json_with_the_resolved_section_and_clean_tags() {
    let garden = Garden::new("note-add-json");
    let run = garden
        .wb(&[
            "note",
            "add",
            "--title",
            "Watering plan",
            "--section",
            "Beds / Spring",
            "--tag",
            "plan",
            "--tag",
            "Watering",
            "--body",
            "Water at dawn.",
            "--json",
        ])
        .ok();
    let id = text_of(&run.json(), "id");
    assert_eq!(
        garden.normalise(&run.stdout, &[(WATER, &id)]),
        added_json(
            WATER,
            "watering-plan-nWATER0.md",
            &json!("Beds / Spring"),
            &["plan", "watering"]
        )
    );
    let by_id = garden
        .wb(&[
            "note",
            "add",
            "--title",
            "By id",
            "--section",
            &garden.beds,
            "--body",
            "x",
            "--json",
        ])
        .ok();
    assert_eq!(by_id.json()["section"], "Beds");
}

#[test]
fn note_add_reads_the_body_from_stdin_or_a_file() {
    let garden = Garden::new("note-add-body");
    let stdin = garden
        .wb_with_stdin(
            &["note", "add", "--section", "seed trays", "--json"],
            "# Seed order\n\nOrder seeds before March.",
        )
        .ok();
    let id = text_of(&stdin.json(), "id");
    assert_eq!(
        garden.normalise(&stdin.stdout, &[("nSEEDS0", &id)]),
        added_json(
            "nSEEDS0",
            "seed-order-nSEEDS0.md",
            &json!("Seed trays"),
            &[]
        )
    );
    assert_eq!(
        garden.scratch.read(&format!("garden/seed-order-{id}.md")),
        format!(
            "---\nid: {id}\nsection: Seed trays\n---\n\n# Seed order\n\nOrder seeds before March.\n"
        )
    );

    let dash = garden
        .wb_with_stdin(
            &[
                "note",
                "add",
                "--title",
                "Compost",
                "--file",
                "-",
                "--section",
                "none",
                "--json",
            ],
            "From stdin via --file -.",
        )
        .ok();
    let dash_id = text_of(&dash.json(), "id");
    assert_eq!(dash.json()["section"], Value::Null);
    assert!(
        garden
            .scratch
            .read(&format!("garden/compost-{dash_id}.md"))
            .ends_with("# Compost\n\nFrom stdin via --file -.\n")
    );

    let body = garden.scratch.path().join("body.md");
    fs::write(&body, "Body read from a file.").expect("body file");
    let file = garden
        .wb(&[
            "note",
            "add",
            "--title",
            "Mulch",
            "--file",
            &body.display().to_string(),
            "--json",
        ])
        .ok();
    let file_id = text_of(&file.json(), "id");
    assert!(
        garden
            .scratch
            .read(&format!("garden/mulch-{file_id}.md"))
            .ends_with("# Mulch\n\nBody read from a file.\n")
    );

    let missing = garden.wb(&["note", "add", "--title", "Mulch", "--file", "nope.md"]);
    assert_eq!(
        (missing.code, missing.stderr.as_str()),
        (
            1,
            "Could not read nope.md: No such file or directory (os error 2).\n"
        )
    );
    let both = garden.wb(&[
        "note", "add", "--title", "X", "--body", "a", "--file", "nope.md",
    ]);
    assert_eq!(both.code, 2);
    assert!(
        both.stderr
            .starts_with("error: the argument '--body <BODY>' cannot be used with '--file <PATH>'"),
        "{}",
        both.stderr
    );
}

#[test]
fn note_add_keeps_where_the_note_came_from() {
    let garden = Garden::new("note-add-source");
    let id = garden.add_note(&[
        "--title",
        "Watering plan",
        "--section",
        "Beds / Spring",
        "--tag",
        "plan",
        "--source",
        "garden diary",
        "--body",
        "Water at dawn.",
    ]);
    assert_eq!(
        garden
            .scratch
            .read(&format!("garden/watering-plan-{id}.md")),
        format!(
            "---\nid: {id}\nsection: Beds / Spring\ntags: [plan]\nsource: garden diary\n---\n\n# Watering plan\n\nWater at dawn.\n"
        )
    );
    let text = garden.wb(&["note", "show", &id]).ok();
    assert!(
        text.stdout.contains("\nsource:   garden diary\n"),
        "{}",
        text.stdout
    );
    let json = garden.wb(&["note", "show", &id, "--json"]).ok().json();
    assert_eq!(json["source"], "garden diary");
}

#[test]
fn note_add_refuses_a_tag_that_is_not_well_formed_and_writes_nothing() {
    let garden = Garden::new("note-add-bad-tag");
    let before = garden.scratch.tree();
    let run = garden.wb(&[
        "note", "add", "--title", "Bad", "--tag", "plan", "--tag", "Plan!", "--body", "x",
    ]);
    assert_eq!(run.code, 1);
    assert_eq!(run.stdout, "");
    assert_eq!(
        run.stderr,
        "\"Plan!\" is not a well-formed tag: use lowercase words joined by -, with / for a namespace, like plan or ticket/abc-12.\n"
    );
    assert_eq!(garden.scratch.tree(), before);
}

#[test]
fn note_add_with_nothing_writes_an_empty_note() {
    let garden = Garden::new("note-add-empty");
    let run = garden.wb(&["note", "add", "--json"]).ok();
    let id = text_of(&run.json(), "id");
    assert_eq!(
        garden.normalise(&run.stdout, &[(EMPTY, &id)]),
        added_json(EMPTY, "note-nEMPTY0.md", &Value::Null, &[])
    );
    assert_eq!(
        garden.scratch.read(&format!("garden/note-{id}.md")),
        format!("---\nid: {id}\n---\n\n")
    );
    let blank = garden
        .wb_with_stdin(&["note", "add", "--title", "Blank", "--json"], "   ")
        .ok();
    let blank_id = text_of(&blank.json(), "id");
    assert_eq!(
        garden.scratch.read(&format!("garden/blank-{blank_id}.md")),
        format!("---\nid: {blank_id}\n---\n\n# Blank\n")
    );
}

#[test]
fn note_add_goes_to_the_default_section_or_to_none_and_refuses_a_missing_one() {
    let garden = Garden::new("note-add-sections");
    let missing = garden.wb(&[
        "note",
        "add",
        "--title",
        "Roses",
        "--section",
        "Shed",
        "--body",
        "x",
    ]);
    assert_eq!(
        (missing.code, missing.stderr.as_str()),
        (
            1,
            "There is no section \"Shed\" on this board; sections: Beds; Beds / Spring; Beds / Spring / Herbs; Seed trays.\n"
        )
    );
    let loose = garden
        .wb(&["note", "add", "--title", "Loose", "--body", "x", "--json"])
        .ok();
    assert_eq!(loose.json()["section"], Value::Null);

    garden.set_default_section(&garden.beds);
    let edging = garden
        .wb(&["note", "add", "--title", "Edging", "--body", "x", "--json"])
        .ok();
    assert_eq!(edging.json()["section"], "Beds");
    let none = garden
        .wb(&[
            "note",
            "add",
            "--title",
            "Compost",
            "--section",
            "None",
            "--json",
        ])
        .ok();
    assert_eq!(none.json()["section"], Value::Null);
    let compost = text_of(&none.json(), "id");
    assert_eq!(
        garden.scratch.read(&format!("garden/compost-{compost}.md")),
        format!("---\nid: {compost}\n---\n\n# Compost\n")
    );
    let notes = garden.wb(&["note", "list", "--json"]).ok().json();
    assert_eq!(notes.as_array().map(Vec::len), Some(3));
}

fn plant_list(garden: &Garden) -> String {
    garden.add_note(&[
        "--title",
        "Plant list",
        "--section",
        "Beds",
        "--tag",
        "plan",
        "--body",
        "Tomatoes, basil and beans.",
    ])
}

#[test]
fn note_show_prints_the_note() {
    let garden = Garden::new("note-show");
    let id = plant_list(&garden);
    let text = garden.wb(&["note", "show", &id]).ok();
    assert_eq!(
        garden.normalise(&text.stdout, &[(PLANT, &id)]),
        "nPLANT0  Plant list
board:    garden
file:     <tmp>/ws/garden/plant-list-nPLANT0.md
section:  Beds
tags:     plan

Tomatoes, basil and beans.

Backlinks: none
"
    );
    let json = garden.wb(&["note", "show", &id, "--json"]).ok();
    assert_eq!(
        garden.normalise(&json.stdout, &[(PLANT, &id)]),
        "{
  \"version\": null,
  \"head\": null,
  \"created\": null,
  \"stack\": null,
  \"order\": null,
  \"id\": \"nPLANT0\",
  \"board\": \"garden\",
  \"path\": \"<tmp>/ws/garden/plant-list-nPLANT0.md\",
  \"title\": \"Plant list\",
  \"body\": \"Tomatoes, basil and beans.\",
  \"section\": \"Beds\",
  \"tags\": [
    \"plan\"
  ],
  \"source\": \"\",
  \"kind\": null,
  \"opens\": null,
  \"extra\": {},
  \"links\": [],
  \"backlinks\": [],
  \"attachments\": []
}
"
    );
}

#[test]
fn note_show_of_an_untitled_note_and_of_a_missing_one() {
    let garden = Garden::new("note-show-empty");
    let id = garden.add_note(&[]);
    let text = garden.wb(&["note", "show", &id]).ok();
    assert_eq!(
        garden.normalise(&text.stdout, &[(EMPTY, &id)]),
        "nEMPTY0  (no title)
board:    garden
file:     <tmp>/ws/garden/note-nEMPTY0.md
section:  -
tags:     -

Backlinks: none
"
    );
    for missing in ["n99", "not an id"] {
        let run = garden.wb(&["note", "show", missing]);
        assert_eq!(
            (run.code, run.stderr),
            (
                1,
                format!(
                    "There is no note {missing} on board garden; `wb note list` shows the notes there are.\n"
                )
            )
        );
    }
    let other = garden
        .scratch
        .wb(&["note", "show", &format!("garden#{id}"), "--json"])
        .ok();
    assert_eq!(other.json()["id"], id.as_str());
}

#[test]
fn note_list_sorts_by_section_then_title_and_filters() {
    let garden = Garden::new("note-list");
    let empty = garden.wb(&["note", "list"]).ok();
    assert_eq!(empty.stdout, "No notes.\n");
    assert_eq!(garden.wb(&["note", "list", "--json"]).ok().stdout, "[]\n");

    let plant = plant_list(&garden);
    let water = garden.add_note(&[
        "--title",
        "Watering plan",
        "--section",
        "Beds / Spring",
        "--tag",
        "plan",
        "--tag",
        "watering",
    ]);
    let untitled = garden.add_note(&[]);
    let loose = garden.add_note(&["--title", "Loose"]);
    let edging = garden.add_note(&["--title", "edging", "--section", "beds"]);
    let seeds = garden.add_note(&["--title", "Seed order", "--section", "Seed trays"]);
    let ids = [
        (PLANT, plant.as_str()),
        (WATER, water.as_str()),
        (EMPTY, untitled.as_str()),
        ("nLOOSE0", loose.as_str()),
        ("nEDGE00", edging.as_str()),
        ("nSEEDS0", seeds.as_str()),
    ];
    let list = |args: &[&str]| {
        let mut all = vec!["note", "list"];
        all.extend_from_slice(args);
        garden.normalise(&garden.wb(&all).ok().stdout, &ids)
    };
    assert_eq!(
        list(&[]),
        "ID       TITLE          SECTION        TAGS
nEMPTY0                 -              -
nLOOSE0  Loose          -              -
nEDGE00  edging         Beds           -
nPLANT0  Plant list     Beds           plan
nWATER0  Watering plan  Beds / Spring  plan, watering
nSEEDS0  Seed order     Seed trays     -
"
    );
    assert_eq!(
        list(&["--section", "Beds"]),
        "ID       TITLE          SECTION        TAGS
nEDGE00  edging         Beds           -
nPLANT0  Plant list     Beds           plan
nWATER0  Watering plan  Beds / Spring  plan, watering
"
    );
    assert_eq!(
        list(&["--section", "Beds", "--exact"]),
        "ID       TITLE       SECTION  TAGS
nEDGE00  edging      Beds     -
nPLANT0  Plant list  Beds     plan
"
    );
    assert_eq!(
        list(&["--tag", "plan", "--tag", "Watering"]),
        "ID       TITLE          SECTION        TAGS
nWATER0  Watering plan  Beds / Spring  plan, watering
"
    );
    let json = garden
        .wb(&["note", "list", "--section", &garden.trays, "--json"])
        .ok();
    assert_eq!(
        garden.normalise(&json.stdout, &ids),
        "[
  {
    \"id\": \"nSEEDS0\",
    \"title\": \"Seed order\",
    \"section\": \"Seed trays\",
    \"stack\": null,
    \"order\": null,
    \"tags\": [],
    \"path\": \"<tmp>/ws/garden/seed-order-nSEEDS0.md\"
  }
]
"
    );
    let exact = garden.wb(&["note", "list", "--exact"]);
    assert_eq!(exact.code, 2);
    let missing = garden.wb(&["note", "list", "--section", "Shed2"]);
    assert_eq!(
        (missing.code, missing.stderr.as_str()),
        (
            1,
            "There is no section \"Shed2\" on this board; sections: Beds; Beds / Spring; Beds / Spring / Herbs; Seed trays.\n"
        )
    );
}

#[test]
fn note_list_labels_an_untitled_note_by_its_body_and_leaves_out_rules() {
    let garden = Garden::new("note-list-label");
    let long = "word ".repeat(20);
    let id = garden.add_note(&["--body", &long]);
    let rules = garden.scratch.workspace().join("garden/rules-nrules01.md");
    fs::write(
        &rules,
        "---\nid: nrules01\nkind: rules\n---\n\n# Rules\n\nBe kind.\n",
    )
    .expect("rules");
    let rows = garden.wb(&["note", "list", "--json"]).ok().json();
    assert_eq!(rows.as_array().map(Vec::len), Some(1));
    assert_eq!(rows[0]["id"], id.as_str());
    assert_eq!(
        rows[0]["title"],
        "word word word word word word word word word word word word…"
    );
    let all = garden
        .wb(&["note", "list", "--rules", "--json"])
        .ok()
        .json();
    assert_eq!(all.as_array().map(Vec::len), Some(2));
    let shown = garden.wb(&["board", "show", "--json"]).ok().json();
    assert_eq!(shown["rules"]["id"], "nrules01");
    assert_eq!(
        (shown["notes"].clone(), shown["unsectioned"].clone()),
        (json!(2), json!(1))
    );
}

#[test]
fn note_edit_changes_the_title_body_and_file_name() {
    let garden = Garden::new("note-edit");
    let id = plant_list(&garden);
    let ids = [(PLANT, id.as_str())];
    let title = garden
        .wb(&["note", "edit", &id, "--title", "Plant list, spring"])
        .ok();
    assert_eq!(
        garden.normalise(&title.stdout, &ids),
        "Changed nPLANT0: <tmp>/ws/garden/plant-list-spring-nPLANT0.md\n"
    );
    let body = garden
        .wb(&[
            "note",
            "edit",
            &id,
            "--body",
            "Tomatoes and beans.",
            "--json",
        ])
        .ok();
    assert_eq!(
        garden.normalise(&body.stdout, &ids),
        "{\n  \"id\": \"nPLANT0\",\n  \"path\": \"<tmp>/ws/garden/plant-list-spring-nPLANT0.md\",\n  \"title\": \"Plant list, spring\"\n}\n"
    );
    let append = garden
        .wb_with_stdin(&["note", "edit", &id, "--append"], "Add peas.")
        .ok();
    assert_eq!(
        garden.normalise(&append.stdout, &ids),
        "Changed nPLANT0: <tmp>/ws/garden/plant-list-spring-nPLANT0.md\n"
    );
    let heading = garden
        .wb(&["note", "edit", &id, "--append", "--body", "# Later\nText."])
        .ok();
    assert_eq!(heading.code, 0);
    let files: Vec<String> = garden
        .scratch
        .tree()
        .into_keys()
        .filter(|name| name.contains(&id) && !name.contains("/.locks/"))
        .collect();
    assert_eq!(files, [format!("garden/plant-list-spring-{id}.md")]);
    assert_eq!(
        garden.scratch.read(&files[0]),
        format!(
            "---\nid: {id}\nsection: Beds\ntags: [plan]\n---\n\n# Plant list, spring\n\nTomatoes and beans.\n\nAdd peas.\n\n# Later\nText.\n"
        )
    );
    let shown = garden.wb(&["note", "show", &id]).ok();
    assert_eq!(
        garden.normalise(&shown.stdout, &ids),
        "nPLANT0  Plant list, spring
board:    garden
file:     <tmp>/ws/garden/plant-list-spring-nPLANT0.md
section:  Beds
tags:     plan

Tomatoes and beans.

Add peas.

# Later
Text.

Backlinks: none
"
    );
}

#[test]
fn note_edit_appending_to_an_empty_note_names_its_file_after_the_text() {
    let garden = Garden::new("note-edit-empty");
    let id = garden.add_note(&[]);
    let run = garden
        .wb(&[
            "note",
            "edit",
            &id,
            "--append",
            "--body",
            "Now it has text.",
            "--json",
        ])
        .ok();
    assert_eq!(
        garden.normalise(&run.stdout, &[(EMPTY, &id)]),
        "{\n  \"id\": \"nEMPTY0\",\n  \"path\": \"<tmp>/ws/garden/now-it-has-text-nEMPTY0.md\",\n  \"title\": \"\"\n}\n"
    );
    assert!(
        !garden
            .scratch
            .workspace()
            .join(format!("garden/note-{id}.md"))
            .exists()
    );
}

#[test]
fn note_edit_refuses_nothing_to_change_and_a_missing_note() {
    let garden = Garden::new("note-edit-errors");
    let id = plant_list(&garden);
    let notes_and_config = || {
        let mut tree = garden.scratch.tree();
        tree.retain(|name, _| !name.contains("/.locks/"));
        tree
    };
    let before = notes_and_config();
    let nothing = "Nothing to change: pass --title, or text with --body, --file or on stdin.\n";
    for (args, stdin) in [
        (vec!["note", "edit", id.as_str()], None),
        (vec!["note", "edit", id.as_str(), "--append"], None),
        (vec!["note", "edit", id.as_str()], Some("   ")),
    ] {
        let run = match stdin {
            Some(text) => garden.wb_with_stdin(&args, text),
            None => garden.wb(&args),
        };
        assert_eq!((run.code, run.stderr.as_str()), (1, nothing), "{args:?}");
    }
    let missing = garden.wb(&["note", "edit", "n99", "--title", "X"]);
    assert_eq!(
        (missing.code, missing.stderr.as_str()),
        (
            1,
            "There is no note n99 on board garden; `wb note list` shows the notes there are.\n"
        )
    );
    assert_eq!(notes_and_config(), before);
}

#[test]
fn note_edit_reaches_a_note_on_another_board() {
    let garden = Garden::new("note-edit-other");
    let id = plant_list(&garden);
    garden.scratch.wb(&["board", "add", "Kitchen"]).ok();
    let run = garden
        .scratch
        .wb(&[
            "--board",
            "kitchen",
            "note",
            "edit",
            &format!("garden#{id}"),
            "--title",
            "Plant list, spring",
        ])
        .ok();
    assert!(
        run.stdout
            .ends_with(&format!("plant-list-spring-{id}.md\n")),
        "{}",
        run.stdout
    );
}

#[test]
fn note_edit_leaves_the_board_file_alone() {
    let garden = Garden::new("note-edit-board-file");
    let id = plant_list(&garden);
    let board_file = garden.scratch.read("garden/board.json");
    garden.wb(&["note", "edit", &id, "--title", "Renamed"]).ok();
    assert_eq!(garden.scratch.read("garden/board.json"), board_file);
}

#[test]
fn twenty_processes_appending_to_one_note_all_land() {
    let garden = Garden::new("note-writers");
    let id = garden.add_note(&["--title", "Plant list"]);
    let children: Vec<_> = (0..20)
        .map(|number| {
            let mut command = garden.scratch.command();
            command
                .arg("--workspace")
                .arg(garden.scratch.workspace())
                .args([
                    "--board", "garden", "note", "edit", &id, "--append", "--body",
                ])
                .arg(format!("line {number}"));
            std::thread::spawn(move || common::run(command, None))
        })
        .collect();
    for child in children {
        child.join().expect("a writer").ok();
    }
    let body = text_of(
        &garden.wb(&["note", "show", &id, "--json"]).ok().json(),
        "body",
    );
    let lines = body
        .lines()
        .filter(|line| line.starts_with("line "))
        .count();
    assert_eq!(
        lines, 20,
        "{lines} of 20 lines appended by processes are in the note:\n{body}"
    );
}
