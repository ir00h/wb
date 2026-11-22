mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::{Scratch, add_note, add_section, append, set_body, set_title};
use wb_core::Workspace;
use wb_model::{CONFIG_FILE, Outcome};

const IMAGE_SIZE: &str = "64m";
const VOLUME_NAME: &str = "GARDEN";

struct Mounted {
    device: String,
    point: PathBuf,
}

impl Drop for Mounted {
    fn drop(&mut self) {
        let detached = hdiutil(&["detach", &self.device]);
        if !detached.status.success() {
            hdiutil(&["detach", "-force", &self.device]);
        }
    }
}

fn hdiutil(args: &[&str]) -> Output {
    Command::new("hdiutil")
        .args(args)
        .output()
        .expect("hdiutil runs")
}

fn hdiutil_is_there() -> bool {
    Command::new("hdiutil").arg("help").output().is_ok()
}

fn said(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn mount(folder: &Path, file_system: &str) -> Mounted {
    let image = folder.join("volume.dmg");
    let point = folder.join("mounted");
    fs::create_dir_all(&point).expect("mount point");
    let created = hdiutil(&[
        "create",
        "-size",
        IMAGE_SIZE,
        "-fs",
        file_system,
        "-volname",
        VOLUME_NAME,
        &image.display().to_string(),
    ]);
    assert!(
        created.status.success(),
        "could not create a {file_system} disk image: {}",
        said(&created)
    );
    let attached = hdiutil(&[
        "attach",
        "-nobrowse",
        "-noverify",
        "-mountpoint",
        &point.display().to_string(),
        &image.display().to_string(),
    ]);
    assert!(
        attached.status.success(),
        "could not attach the {file_system} disk image, so the volume test did not run: {}",
        said(&attached)
    );
    let device = String::from_utf8_lossy(&attached.stdout)
        .split_whitespace()
        .next()
        .filter(|word| word.starts_with("/dev/disk"))
        .map_or_else(
            || panic!("no device in the answer of hdiutil: {}", said(&attached)),
            str::to_owned,
        );
    Mounted { device, point }
}

fn a_board_keeps_exactly_what_was_written(file_system: &str) {
    let scratch = Scratch::new(&format!("volume-{}", file_system.replace(' ', "-")));
    let mounted = mount(scratch.path(), file_system);
    let root = mounted.point.join("workspace");
    fs::create_dir(&root).expect("workspace folder");
    let workspace = Workspace::open(&root).expect("workspace");

    let garden = workspace
        .add_board("Garden", None)
        .unwrap_or_else(|error| panic!("{file_system}: adding a board answered {error:?}"));
    let board = workspace.board(&garden.id).expect("board handle");
    board
        .apply(add_section("Beds"))
        .unwrap_or_else(|error| panic!("{file_system}: adding a section answered {error:?}"));
    let note = add_note(&board, "Peas", "Sown.");
    for line in ["Watered.", "Weeded."] {
        let answer = board.apply(append(&note.id, line));
        assert!(
            matches!(answer, Ok(Outcome::Applied { .. })),
            "{file_system}: appending {line:?} answered {answer:?}"
        );
    }
    let edited = board.apply(set_body(
        &note.id,
        Some("Sown.\n\nWatered.\n\nWeeded."),
        "Sown, watered and weeded.",
    ));
    assert!(
        matches!(edited, Ok(Outcome::Applied { .. })),
        "{file_system}: the edit answered {edited:?}"
    );
    let renamed = board.apply(set_title(&note.id, Some("Peas"), "Sugar peas"));
    assert!(
        matches!(renamed, Ok(Outcome::Applied { .. })),
        "{file_system}: the title change answered {renamed:?}"
    );

    let read_back = board.note(&note.id).expect("note");
    assert_eq!(
        (read_back.title.as_str(), read_back.body.as_str()),
        ("Sugar peas", "Sown, watered and weeded."),
        "{file_system}: the note does not read back as written"
    );
    let config = fs::read_to_string(root.join("garden").join(CONFIG_FILE)).expect("config");
    assert_eq!(
        config.matches("\"name\": \"Beds\"").count(),
        1,
        "{file_system}: the config reads\n{config}"
    );
    let notes = board.notes().expect("notes");
    assert_eq!(notes.len(), 1, "{file_system}: the board holds {notes:?}");
    drop(mounted);
}

#[test]
fn a_board_on_a_fat32_volume_keeps_exactly_what_was_written() {
    if !hdiutil_is_there() {
        println!("hdiutil is missing, so no FAT32 volume was made and this test checked nothing");
        return;
    }
    a_board_keeps_exactly_what_was_written("MS-DOS FAT32");
}

#[test]
fn a_board_on_an_exfat_volume_keeps_exactly_what_was_written() {
    if !hdiutil_is_there() {
        println!("hdiutil is missing, so no exFAT volume was made and this test checked nothing");
        return;
    }
    a_board_keeps_exactly_what_was_written("ExFAT");
}
