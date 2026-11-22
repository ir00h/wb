mod common;

use std::fs;

use common::{BOARD_ENV, Garden, Scratch, WORKSPACE_ENV, run};

const EMPTY_BOARD_FILE: &str =
    "{\n  \"version\": 2,\n  \"name\": \"garden\",\n  \"cards\": {},\n  \"sections\": {}\n}";

#[test]
fn without_a_workspace_wb_refuses_and_touches_nothing() {
    let scratch = Scratch::new("no-workspace");
    for args in [
        vec!["board", "list"],
        vec!["board", "add", "garden"],
        vec!["--board", "garden", "note", "add", "--title", "Plant list"],
    ] {
        let mut command = scratch.command();
        command.args(&args);
        let run = run(command, None);
        assert_eq!(
            (run.code, run.stdout.as_str(), run.stderr.as_str()),
            (
                1,
                "",
                "No workspace given: pass --workspace <path> or set WHITEBOARD_WORKSPACE.\n"
            ),
            "{args:?}"
        );
    }
    let left: Vec<_> = fs::read_dir(scratch.path()).expect("scratch").collect();
    assert!(left.is_empty(), "wb wrote {left:?}");
}

#[test]
fn the_workspace_and_board_can_come_from_the_environment_and_the_flags_win() {
    let garden = Garden::new("env");
    let workspace = garden.scratch.workspace();
    let mut command = garden.scratch.command();
    command
        .env(WORKSPACE_ENV, &workspace)
        .env(BOARD_ENV, " /garden/ ")
        .args(["section", "list", "--json"]);
    let rows = run(command, None).ok().json();
    assert_eq!(rows.as_array().map(Vec::len), Some(4));

    let mut command = garden.scratch.command();
    command
        .env(WORKSPACE_ENV, garden.scratch.path().join("elsewhere"))
        .env(BOARD_ENV, "nowhere")
        .arg("--workspace")
        .arg(&workspace)
        .args(["--board", "garden", "section", "list", "--json"]);
    assert_eq!(run(command, None).ok().json(), rows);
}

#[test]
fn a_workspace_that_does_not_exist_is_refused() {
    let scratch = Scratch::new("missing-workspace");
    for args in [&["board", "list"][..], &["board", "list", "--json"]] {
        let run = scratch.wb_in(&scratch.path().join("nows"), args, None);
        assert_eq!(run.code, 1);
        assert_eq!(run.stdout, "");
        assert_eq!(
            scratch.normalise(&run.stderr, &[]),
            "There is no workspace at <tmp>/nows; pass --workspace, set WHITEBOARD_WORKSPACE, or create a board with `wb board add`.\n"
        );
    }
}

#[test]
fn board_add_makes_the_workspace_and_an_empty_board_file() {
    let scratch = Scratch::new("board-add");
    let run = scratch.wb(&["board", "add", "garden"]).ok();
    assert_eq!(
        scratch.normalise(&run.stdout, &[]),
        "garden\n<tmp>/ws/garden\n"
    );
    let tree = scratch.tree();
    assert_eq!(
        tree.keys().collect::<Vec<_>>(),
        ["garden/.locks/board.config.lock", "garden/board.json"]
    );
    assert_eq!(tree["garden/board.json"].trim_end(), EMPTY_BOARD_FILE);
}

#[test]
fn board_add_prints_json_and_numbers_a_second_board_of_the_same_name() {
    let scratch = Scratch::new("board-add-json");
    let first = scratch
        .wb(&["board", "add", "  Kitchen   Table ", "--json"])
        .ok();
    assert_eq!(
        scratch.normalise(&first.stdout, &[]),
        "{\n  \"id\": \"kitchen-table\",\n  \"name\": \"Kitchen Table\",\n  \"path\": \"<tmp>/ws/kitchen-table\",\n  \"card\": null\n}\n"
    );
    scratch.wb(&["board", "add", "garden"]).ok();
    let again = scratch.wb(&["board", "add", "garden", "--json"]).ok();
    assert_eq!(
        scratch.normalise(&again.stdout, &[]),
        "{\n  \"id\": \"garden-2\",\n  \"name\": \"garden\",\n  \"path\": \"<tmp>/ws/garden-2\",\n  \"card\": null\n}\n"
    );
    let nested = scratch
        .wb(&[
            "board",
            "add",
            "Spice rack",
            "--parent",
            "kitchen-table",
            "--json",
        ])
        .ok();
    assert_eq!(nested.json()["id"], "kitchen-table/spice-rack");
    assert!(
        scratch
            .workspace()
            .join("kitchen-table/spice-rack/board.json")
            .is_file()
    );
}

#[test]
fn board_add_refuses_an_empty_name_and_a_missing_parent() {
    let scratch = Scratch::new("board-add-errors");
    let empty = scratch.wb(&["board", "add", "   "]);
    assert_eq!(
        (empty.code, empty.stderr.as_str()),
        (1, "A board needs a name.\n")
    );
    assert!(!scratch.workspace().exists());
    scratch.wb(&["board", "add", "garden"]).ok();
    let missing = scratch.wb(&["board", "add", "Recipes", "--parent", "nowhere"]);
    assert_eq!(
        (missing.code, missing.stderr.as_str()),
        (1, "There is no board nowhere; boards: garden.\n")
    );
    let unknown = scratch.wb(&["board", "create", "garden"]);
    assert_eq!(unknown.code, 2);
}

#[test]
fn board_list_shows_every_board_with_its_notes() {
    let scratch = Scratch::new("board-list");
    let empty = scratch.wb_in(scratch.path(), &["board", "list"], None).ok();
    assert_eq!(empty.stdout, "No boards.\n");
    let empty = scratch
        .wb_in(scratch.path(), &["board", "list", "--json"], None)
        .ok();
    assert_eq!(empty.stdout, "[]\n");

    scratch.wb(&["board", "add", "garden"]).ok();
    scratch.wb(&["board", "add", "Kitchen Table"]).ok();
    scratch
        .wb(&["--board", "garden", "note", "add", "--title", "Plant list"])
        .ok();
    let text = scratch.wb(&["board", "list"]).ok();
    assert_eq!(
        text.stdout,
        "ID             NAME           NOTES  HUE  REPOS\ngarden         garden         1      -    -\nkitchen-table  Kitchen Table  0      -    -\n"
    );
    let json = scratch.wb(&["board", "list", "--json"]).ok();
    assert_eq!(
        json.stdout,
        "[\n  {\n    \"id\": \"garden\",\n    \"name\": \"garden\",\n    \"notes\": 1,\n    \"repos\": [],\n    \"hue\": null\n  },\n  {\n    \"id\": \"kitchen-table\",\n    \"name\": \"Kitchen Table\",\n    \"notes\": 0,\n    \"repos\": [],\n    \"hue\": null\n  }\n]\n"
    );
}

#[test]
fn board_list_keeps_listing_when_a_board_has_a_file_it_cannot_read() {
    let scratch = Scratch::new("board-list-broken");
    scratch.wb(&["board", "add", "garden"]).ok();
    scratch.wb(&["board", "add", "Kitchen"]).ok();
    scratch
        .wb(&["--board", "kitchen", "note", "add", "--title", "Bread"])
        .ok();
    fs::write(
        scratch.workspace().join("garden/board.config.json"),
        "{\"hue\": 120, \"repos\": [\"/srv/git/garden\"]}",
    )
    .expect("config written");
    fs::write(
        scratch.workspace().join("kitchen/board.config.json"),
        "{\"sections\": [",
    )
    .expect("config written");
    assert_eq!(
        scratch.wb(&["board", "list"]).ok().stdout,
        "ID       NAME     NOTES  HUE  REPOS\ngarden   garden   0      120  /srv/git/garden\nkitchen  Kitchen  1      -    -\n"
    );
    fs::write(scratch.workspace().join("kitchen/board.json"), "{").expect("board file written");
    assert_eq!(
        scratch.wb(&["board", "list"]).ok().stdout,
        "ID       NAME     NOTES  HUE  REPOS\ngarden   garden   0      120  /srv/git/garden\nkitchen  kitchen  1      -    -\n"
    );
    assert_eq!(
        scratch.wb(&["board", "list", "--json"]).ok().json()[1],
        serde_json::json!({"id": "kitchen", "name": "kitchen", "notes": 1, "repos": [], "hue": null})
    );
}

#[test]
fn board_show_gives_sections_tags_and_settings() {
    let garden = Garden::new("board-show");
    garden.set_default_section(&garden.beds);
    garden.add_note(&[
        "--title",
        "Plant list",
        "--section",
        "Beds",
        "--tag",
        "plan",
    ]);
    garden.add_note(&[
        "--title",
        "Watering plan",
        "--section",
        "Beds / Spring",
        "--tag",
        "plan",
        "--tag",
        "watering",
    ]);
    garden.add_note(&[
        "--title",
        "Seed order",
        "--section",
        "Seed trays",
        "--tag",
        "watering",
        "--tag",
        "beans",
    ]);
    let text = garden.wb(&["board", "show"]).ok();
    assert_eq!(
        garden.normalise(&text.stdout, &[]),
        "garden (garden)
3 notes, 0 in no section
<tmp>/ws/garden

SECTION     NOTES  ID       ROLE     LAYOUT  NEXT
Beds        1      sBEDS00  default  -       -
  Spring    1      sSPRING           -       -
    Herbs   0      sHERBS0           -       -
Seed trays  1      sTRAYS0           -       -

Tags: plan (2), watering (2), beans (1)
Default section: Beds
Archive section: -
Automatic tags: -
Repositories: -
Colour: -
Layout: -
Rules: none; `wb rules init` writes a first version
"
    );
    let json = garden
        .scratch
        .wb(&["board", "show", "garden", "--json"])
        .ok()
        .json();
    let keys: Vec<&String> = json.as_object().expect("object").keys().collect();
    assert_eq!(
        keys,
        [
            "id",
            "name",
            "path",
            "notes",
            "unsectioned",
            "sections",
            "tags",
            "defaultSection",
            "archiveSection",
            "autoTags",
            "repos",
            "hue",
            "layout",
            "rules"
        ]
    );
    assert_eq!(json["defaultSection"], "Beds");
    assert_eq!(json["sections"][0]["default"], true);
    assert_eq!(
        json["tags"],
        serde_json::json!([
            {"name": "plan", "notes": 2},
            {"name": "watering", "notes": 2},
            {"name": "beans", "notes": 1}
        ])
    );
    assert_eq!(json["rules"], serde_json::Value::Null);
}

#[test]
fn board_show_names_a_default_or_archive_section_the_config_lacks_as_missing() {
    let scratch = Scratch::new("board-show-missing-default");
    scratch.wb(&["board", "add", "garden"]).ok();
    let shown = |default: &str, archive: &str| {
        fs::write(
            scratch.workspace().join("garden/board.config.json"),
            serde_json::json!({
                "sections": [{"id": "sbeds00", "name": "Beds"}],
                "defaultSection": default,
                "archiveSection": archive,
            })
            .to_string(),
        )
        .expect("config written");
        let text = scratch.wb(&["--board", "garden", "board", "show"]).ok();
        let json = scratch
            .wb(&["--board", "garden", "board", "show", "--json"])
            .ok()
            .json();
        let lines: Vec<String> = text
            .stdout
            .lines()
            .filter(|line| line.contains(" section: "))
            .map(str::to_owned)
            .collect();
        (
            lines,
            json["defaultSection"].clone(),
            json["archiveSection"].clone(),
        )
    };
    let expected = |default: &str, archive: &str| {
        (
            vec![
                format!("Default section: {default}"),
                format!("Archive section: {archive}"),
            ],
            serde_json::json!(default),
            serde_json::json!(archive),
        )
    };
    assert_eq!(
        shown("sgone01", "sbeds00"),
        expected("sgone01 (missing)", "Beds")
    );
    assert_eq!(
        shown("sbeds00", "sgone02"),
        expected("Beds", "sgone02 (missing)")
    );
    assert_eq!(
        shown("Old beds", "sbeds00"),
        expected("Old beds (missing)", "Beds")
    );
}

#[test]
fn no_line_of_a_table_ends_in_a_space() {
    let garden = Garden::new("trailing-space");
    fs::write(
        garden.scratch.workspace().join("garden/board.config.json"),
        "{\"sections\": [{\"id\": \"sbeds00\", \"name\": \"Beds \", \"next\": \"sbeds00\"}], \"repos\": [\"\"]}",
    )
    .expect("config written");
    garden.add_note(&["--title", "Plant list", "--tag", "plan"]);
    for args in [
        &["board", "list"][..],
        &["section", "list"],
        &["note", "list"],
    ] {
        let run = garden.wb(args).ok();
        let spaced: Vec<&str> = run
            .stdout
            .lines()
            .filter(|line| line.ends_with(char::is_whitespace))
            .collect();
        assert!(spaced.is_empty(), "{args:?} printed {spaced:?}");
    }
}

#[test]
fn board_show_without_sections_and_its_errors() {
    let scratch = Scratch::new("board-show-errors");
    scratch.wb(&["board", "add", "garden"]).ok();
    let shown = scratch.wb(&["board", "show", "garden"]).ok();
    assert_eq!(
        scratch.normalise(&shown.stdout, &[]),
        "garden (garden)\n0 notes, 0 in no section\n<tmp>/ws/garden\n\nNo sections.\n\nTags: -\nDefault section: -\nArchive section: -\nAutomatic tags: -\nRepositories: -\nColour: -\nLayout: -\nRules: none; `wb rules init` writes a first version\n"
    );
    let missing = scratch.wb(&["board", "show", "nowhere"]);
    assert_eq!(
        (missing.code, missing.stderr.as_str()),
        (1, "There is no board nowhere; boards: garden.\n")
    );
    let none = scratch.wb(&["board", "show"]);
    assert_eq!(
        (none.code, none.stderr.as_str()),
        (
            1,
            "No board given: pass --board <id> or set WB_BOARD; boards: garden.\n"
        )
    );
}

#[test]
fn the_board_option_is_checked_and_trimmed() {
    let scratch = Scratch::new("board-option");
    let empty = scratch.wb_in(scratch.path(), &["section", "list"], None);
    assert_eq!(
        empty.stderr,
        "No board given: pass --board <id> or set WB_BOARD; the workspace has no boards yet.\n"
    );
    scratch.wb(&["board", "add", "garden"]).ok();
    let bad = scratch.wb(&["--board", "../garden", "section", "list"]);
    assert_eq!(
        (bad.code, bad.stderr.as_str()),
        (
            1,
            "\"../garden\" is not a board id; ids are folder paths like kitchen/recipes; boards: garden.\n"
        )
    );
    let trimmed = scratch
        .wb(&["--board", " /garden/ ", "section", "list"])
        .ok();
    assert_eq!(trimmed.stdout, "No sections.\n");
    let first = scratch
        .wb(&["--json", "--board", "garden", "section", "list"])
        .ok();
    assert_eq!(first.stdout, "[]\n");
    let blank = scratch.wb(&["--board", " / ", "section", "list"]);
    assert_eq!(
        (blank.code, blank.stderr.as_str()),
        (
            1,
            "No board given: pass --board <id> or set WB_BOARD; boards: garden.\n"
        )
    );
}
