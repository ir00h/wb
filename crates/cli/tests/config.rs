mod common;

use std::fs;
use std::path::Path;
use std::thread;
use std::time::Duration;

use common::Garden;

const CONFIG: &str = "garden/board.config.json";
const OUTSIDE_WRITE_DELAY: Duration = Duration::from_millis(40);
const NAMES_ONLY_THE_CORE_USES: [&str; 6] = [
    "CONFIG_FILE",
    "BOARD_FILE",
    "BoardFile",
    "BoardConfig::parse",
    "\"board.json\"",
    "\"board.config.json\"",
];

#[test]
fn a_config_that_is_not_text_gives_every_command_one_sentence() {
    let garden = Garden::new("config-not-text");
    fs::write(
        garden.scratch.workspace().join(CONFIG),
        b"{\"sections\": [{\"id\": \"sbeds00\", \"name\": \"Be\xffds\"}]}",
    )
    .expect("config written");
    let answers: Vec<(i32, String)> = [
        &["section", "list"][..],
        &["section", "add", "Roses"],
        &["board", "show"],
        &["note", "add", "--title", "Plant list", "--section", "Beds"],
    ]
    .iter()
    .map(|args| {
        let run = garden.wb(args);
        (run.code, garden.normalise(&run.stderr, &[]))
    })
    .collect();
    let expected = (
        1,
        "`<tmp>/ws/garden/board.config.json` cannot be read as written: invalid utf-8 sequence of 1 bytes from index 43.\n"
            .to_owned(),
    );
    assert_eq!(answers, vec![expected; 4]);
}

#[test]
fn a_config_an_outside_program_empties_before_writing_it_again_is_waited_out() {
    let garden = Garden::new("config-blank");
    let path = garden.scratch.workspace().join(CONFIG);
    let text = fs::read_to_string(&path).expect("config");
    fs::write(&path, "").expect("config emptied");
    let writer = {
        let path = path.clone();
        thread::spawn(move || {
            thread::sleep(OUTSIDE_WRITE_DELAY);
            fs::write(&path, text).expect("config written again");
        })
    };
    let listed = garden.wb(&["section", "list"]).ok();
    writer.join().expect("the outside writer");
    assert!(
        listed.stdout.starts_with("SECTION "),
        "section list answered from the empty file:\n{}",
        listed.stdout
    );
}

#[test]
fn wb_leaves_reading_the_board_files_to_the_core() {
    let mut folders = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
    let mut found = Vec::new();
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).expect("a source folder") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                folders.push(path);
                continue;
            }
            let text = fs::read_to_string(&path).expect("a source file");
            for name in NAMES_ONLY_THE_CORE_USES {
                if text.contains(name) {
                    found.push(format!("{} names {name}", path.display()));
                }
            }
        }
    }
    assert!(found.is_empty(), "wb reads a board file itself: {found:#?}");
}
