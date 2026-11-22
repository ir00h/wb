mod common;

use common::{Garden, Scratch, text_of};

#[test]
fn section_add_prints_the_id_and_path_or_json() {
    let scratch = Scratch::new("section-add");
    scratch.wb(&["board", "add", "garden"]).ok();
    let text = scratch
        .wb(&["--board", "garden", "section", "add", "Beds"])
        .ok();
    let beds = text.stdout.lines().next().expect("an id").to_owned();
    assert_eq!(
        scratch.normalise(&text.stdout, &[("sBEDS00", &beds)]),
        "sBEDS00\nBeds\n"
    );

    let trays = scratch
        .wb(&[
            "--board",
            "garden",
            "section",
            "add",
            "Seed  trays",
            "--json",
        ])
        .ok();
    let trays_id = text_of(&trays.json(), "id");
    assert_eq!(
        scratch.normalise(&trays.stdout, &[("sTRAYS0", &trays_id)]),
        "{\n  \"id\": \"sTRAYS0\",\n  \"name\": \"Seed trays\",\n  \"path\": \"Seed trays\",\n  \"parent\": null\n}\n"
    );

    let spring = scratch
        .wb(&[
            "--board", "garden", "section", "add", "Spring", "--parent", "Beds", "--json",
        ])
        .ok();
    let spring_id = text_of(&spring.json(), "id");
    assert_eq!(
        scratch.normalise(
            &spring.stdout,
            &[("sBEDS00", &beds), ("sSPRING", &spring_id)]
        ),
        "{\n  \"id\": \"sSPRING\",\n  \"name\": \"Spring\",\n  \"path\": \"Beds / Spring\",\n  \"parent\": \"sBEDS00\"\n}\n"
    );

    let herbs = scratch
        .wb(&[
            "--board",
            "garden",
            "section",
            "add",
            "Herbs",
            "--parent",
            "beds / SPRING",
        ])
        .ok();
    assert!(
        herbs.stdout.ends_with("\nBeds / Spring / Herbs\n"),
        "{}",
        herbs.stdout
    );
    let by_id = scratch
        .wb(&[
            "--board", "garden", "section", "add", "Pots", "--parent", &trays_id, "--json",
        ])
        .ok();
    assert_eq!(by_id.json()["path"], "Seed trays / Pots");
}

#[test]
fn section_add_writes_only_the_config() {
    let garden = Garden::new("section-disk");
    let tree = garden.scratch.tree();
    assert_eq!(
        tree.keys().collect::<Vec<_>>(),
        [
            "garden/.locks/board.config.lock",
            "garden/board.config.json",
            "garden/board.json"
        ]
    );
    assert_eq!(
        garden.normalise(&tree["garden/board.config.json"], &[]),
        "{
  \"version\": 1,
  \"sections\": [
    {
      \"id\": \"sBEDS00\",
      \"name\": \"Beds\"
    },
    {
      \"id\": \"sTRAYS0\",
      \"name\": \"Seed trays\"
    },
    {
      \"id\": \"sSPRING\",
      \"name\": \"Spring\",
      \"parent\": \"sBEDS00\"
    },
    {
      \"id\": \"sHERBS0\",
      \"name\": \"Herbs\",
      \"parent\": \"sSPRING\"
    }
  ]
}
"
    );
}

#[test]
fn section_add_refuses_sibling_names_missing_parents_and_empty_names() {
    let garden = Garden::new("section-errors");
    let cases: [(&[&str], &str); 4] = [
        (
            &["section", "add", "beds"],
            "There is already a section \"Beds\" at the top level (sBEDS00); sibling sections need different names.\n",
        ),
        (
            &["section", "add", "Herbs", "--parent", "Beds / Spring"],
            "There is already a section \"Herbs\" in \"Beds / Spring\" (sHERBS0); sibling sections need different names.\n",
        ),
        (
            &["section", "add", "Roses", "--parent", "Shed"],
            "There is no section \"Shed\" on this board; sections: Beds; Beds / Spring; Beds / Spring / Herbs; Seed trays.\n",
        ),
        (&["section", "add", " "], "A section needs a name.\n"),
    ];
    for (args, message) in cases {
        let run = garden.wb(args);
        assert_eq!(run.code, 1, "{args:?}");
        assert_eq!(run.stdout, "");
        assert_eq!(garden.normalise(&run.stderr, &[]), message);
    }
    let scratch = Scratch::new("section-no-sections");
    scratch.wb(&["board", "add", "garden"]).ok();
    let run = scratch.wb(&[
        "--board", "garden", "section", "add", "Roses", "--parent", "Shed",
    ]);
    assert_eq!(
        run.stderr,
        "There is no section \"Shed\" on this board; the board has no sections yet.\n"
    );
}

#[test]
fn section_add_keeps_a_note_and_a_hue_and_refuses_a_hue_out_of_range() {
    let scratch = Scratch::new("section-note-hue");
    scratch.wb(&["board", "add", "garden"]).ok();
    let added = scratch
        .wb(&[
            "--board",
            "garden",
            "section",
            "add",
            "Seed trays",
            "--note",
            "Started indoors",
            "--hue",
            "120",
            "--json",
        ])
        .ok();
    let trays = text_of(&added.json(), "id");
    assert_eq!(
        scratch.normalise(&added.stdout, &[("sTRAYS0", &trays)]),
        "{\n  \"id\": \"sTRAYS0\",\n  \"name\": \"Seed trays\",\n  \"path\": \"Seed trays\",\n  \"parent\": null\n}\n"
    );
    let config = scratch.read("garden/board.config.json");
    assert_eq!(
        scratch.normalise(&config, &[("sTRAYS0", &trays)]),
        "{
  \"version\": 1,
  \"sections\": [
    {
      \"id\": \"sTRAYS0\",
      \"name\": \"Seed trays\",
      \"note\": \"Started indoors\",
      \"hue\": 120
    }
  ]
}
"
    );
    let rows = scratch
        .wb(&["--board", "garden", "section", "list", "--json"])
        .ok()
        .json();
    assert_eq!(rows[0]["note"], "Started indoors");
    assert_eq!(rows[0]["hue"], serde_json::json!(120.0));

    let before = scratch.tree();
    let out_of_range = scratch.wb(&[
        "--board", "garden", "section", "add", "Roses", "--hue", "400",
    ]);
    assert_eq!(out_of_range.code, 1);
    assert_eq!(out_of_range.stdout, "");
    assert_eq!(
        out_of_range.stderr,
        "A hue is a number from 0 to 360, not 400.\n"
    );
    let not_a_number = scratch.wb(&[
        "--board", "garden", "section", "add", "Roses", "--hue", "red",
    ]);
    assert_eq!(not_a_number.code, 2);
    assert_eq!(
        not_a_number.stderr,
        "error: invalid value 'red' for '--hue <HUE>': invalid float literal\n\nFor more information, try '--help'.\n"
    );
    assert_eq!(scratch.tree(), before);
}

#[test]
fn section_list_shows_the_tree_with_counts() {
    let garden = Garden::new("section-list");
    let empty = Scratch::new("section-list-empty");
    empty.wb(&["board", "add", "garden"]).ok();
    assert_eq!(
        empty
            .wb(&["--board", "garden", "section", "list"])
            .ok()
            .stdout,
        "No sections.\n"
    );
    assert_eq!(
        empty
            .wb(&["--board", "garden", "section", "list", "--json"])
            .ok()
            .stdout,
        "[]\n"
    );

    garden.add_note(&["--title", "Plant list", "--section", "Beds"]);
    garden.add_note(&["--title", "Basil", "--section", "Beds / Spring / Herbs"]);
    garden.add_note(&["--title", "Loose"]);
    let text = garden.wb(&["section", "list"]).ok();
    assert_eq!(
        garden.normalise(&text.stdout, &[]),
        "SECTION     NOTES  ID       ROLE  LAYOUT  NEXT
Beds        1      sBEDS00        -       -
  Spring    0      sSPRING        -       -
    Herbs   1      sHERBS0        -       -
Seed trays  0      sTRAYS0        -       -
"
    );
    let json = garden.wb(&["section", "list", "--json"]).ok();
    let rows = json.json();
    assert_eq!(rows.as_array().map(Vec::len), Some(4));
    assert_eq!(
        garden.normalise(&serde_json::to_string_pretty(&rows[1]).expect("JSON"), &[]),
        "{
  \"id\": \"sSPRING\",
  \"name\": \"Spring\",
  \"path\": \"Beds / Spring\",
  \"parent\": \"sBEDS00\",
  \"depth\": 1,
  \"note\": \"\",
  \"hue\": null,
  \"notes\": 0,
  \"listed\": true,
  \"default\": false,
  \"archive\": false,
  \"layout\": null,
  \"next\": null
}"
    );
    assert_eq!(rows[2]["notes"], 1);
}

#[test]
fn twenty_processes_adding_sections_all_land() {
    let scratch = Scratch::new("section-writers");
    scratch.wb(&["board", "add", "garden"]).ok();
    let children: Vec<_> = (0..20)
        .map(|number| {
            let mut command = scratch.command();
            command.arg("--workspace").arg(scratch.workspace()).args([
                "--board",
                "garden",
                "section",
                "add",
                &format!("Bed {number}"),
            ]);
            std::thread::spawn(move || common::run(command, None))
        })
        .collect();
    for child in children {
        child.join().expect("a writer").ok();
    }
    let rows = scratch
        .wb(&["--board", "garden", "section", "list", "--json"])
        .ok()
        .json();
    let count = rows.as_array().map_or(0, Vec::len);
    assert_eq!(
        count, 20,
        "{count} of 20 sections added by processes are in the config"
    );
}
