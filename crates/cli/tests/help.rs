mod common;

use common::{Scratch, run};

const COMMANDS: [&[&str]; 13] = [
    &[],
    &["board"],
    &["board", "list"],
    &["board", "show"],
    &["board", "add"],
    &["section"],
    &["section", "list"],
    &["section", "add"],
    &["note"],
    &["note", "list"],
    &["note", "show"],
    &["note", "add"],
    &["note", "edit"],
];
const EXAMPLES: &str = "Examples:";
const COLUMNS_ENV: &str = "COLUMNS";
const NARROW: &str = "40";
const WIDE: &str = "400";

fn help(scratch: &Scratch, command: &[&str], columns: &str) -> String {
    let mut wb = scratch.command();
    wb.env(COLUMNS_ENV, columns)
        .arg("--workspace")
        .arg(scratch.workspace())
        .args(command)
        .arg("--help");
    run(wb, None).ok().stdout
}

#[test]
fn the_help_is_the_same_at_any_terminal_width() {
    let scratch = Scratch::new("help-width");
    for command in COMMANDS {
        assert_eq!(
            help(&scratch, command, NARROW),
            help(&scratch, command, WIDE),
            "`wb {}` --help follows the terminal's width",
            command.join(" ")
        );
    }
}

#[test]
fn every_example_in_the_help_is_one_whole_command_on_one_line() {
    let scratch = Scratch::new("help-examples");
    for command in COMMANDS {
        let page = help(&scratch, command, NARROW);
        let Some((_, examples)) = page.split_once(EXAMPLES) else {
            continue;
        };
        for line in examples.lines().filter(|line| !line.is_empty()) {
            assert!(
                line.starts_with("  ") && line.contains("wb --workspace "),
                "`wb {}` --help has an example cut in two at:\n{line}",
                command.join(" ")
            );
        }
    }
}

#[test]
fn note_add_help_says_a_tag_that_is_not_well_formed_is_refused() {
    let scratch = Scratch::new("help-note-add");
    let page = help(&scratch, &["note", "add"], NARROW);
    let words = page.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        words.contains(
            "Refused: a section the config lacks, a tag that is not well formed (lowercase words joined by -, with / for a namespace, like plan or ticket/abc-12)."
        ),
        "{page}"
    );
}
