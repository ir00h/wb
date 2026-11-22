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
const FILE_SYSTEM_PATH: &str = "fs::";
const FILE_HANDLES: [&str; 2] = ["File", "OpenOptions"];
const FILE_SYSTEM_CALLS_WB_MAKES: [&str; 2] = ["fs::create_dir_all", "fs::read_to_string"];

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

fn file_system_calls(text: &str) -> Vec<String> {
    let mut calls: Vec<String> = text
        .match_indices(FILE_SYSTEM_PATH)
        .map(|(at, _)| {
            let name: String = text[at + FILE_SYSTEM_PATH.len()..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            format!("{FILE_SYSTEM_PATH}{name}")
        })
        .collect();
    calls.extend(
        text.split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .filter(|word| FILE_HANDLES.contains(word))
            .map(str::to_owned),
    );
    calls
}

#[test]
fn wb_leaves_reading_the_board_files_to_the_core() {
    let mut folders = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
    let mut found = Vec::new();
    let mut calls = Vec::new();
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
            calls.extend(file_system_calls(&text));
        }
    }
    assert!(found.is_empty(), "wb reads a board file itself: {found:#?}");
    calls.sort();
    assert_eq!(
        calls, FILE_SYSTEM_CALLS_WB_MAKES,
        "wb touches the file system beyond making the workspace folder and reading the body file it is given"
    );
}
