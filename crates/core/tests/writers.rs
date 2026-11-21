mod common;

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, mpsc};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use common::{
    ROLE, Scratch, add_note, add_section, append, files_holding, open_board, set_body, set_title,
    setting, spawn_role, wait_for,
};
use wb_core::lock::{BoardLocks, LockError, LockKey};
use wb_core::{TRASH_FOLDER, Workspace, config};
use wb_model::{Error, NoteId, Op, Outcome, SectionId};

const WRITERS: usize = 20;
const OTHER_PROCESS: &str = "another_process_plays_its_role";
const WORKSPACE: &str = "WB_TEST_WORKSPACE";
const NOTE: &str = "WB_TEST_NOTE";
const NUMBER: &str = "WB_TEST_NUMBER";
const GARDEN: &str = "garden";
const STALL: Duration = Duration::from_secs(6);
const ARRIVAL: Duration = Duration::from_millis(300);
const LONG_DEADLINE: Duration = Duration::from_secs(20);
const SHORT_DEADLINE: Duration = Duration::from_secs(2);
const RACERS: usize = 8;
const RACE_TRIALS: usize = 200;

fn line(number: usize) -> String {
    format!("line {number} of the harvest log")
}

fn section_name(number: usize) -> String {
    format!("Bed {number}")
}

fn ready_marker(workspace: &Path, number: &str) -> PathBuf {
    workspace.join(format!("ready-{number}"))
}

fn start_signal(workspace: &Path) -> PathBuf {
    workspace.join("start")
}

fn writer_op(role: &str, note: &str, number: usize) -> Op {
    match role {
        "append" => append(&NoteId::parse(note).expect("note id"), &line(number)),
        "add-section" => add_section(&section_name(number)),
        other => panic!("no writer role {other}"),
    }
}

#[test]
fn another_process_plays_its_role() {
    let Ok(role) = std::env::var(ROLE) else {
        return;
    };
    let workspace = PathBuf::from(setting(WORKSPACE));
    let number = setting(NUMBER);
    if role == "hold-config" {
        let locks = BoardLocks::of_board(&workspace.join(GARDEN));
        let _held = locks
            .acquire(&[LockKey::Config], LONG_DEADLINE)
            .expect("lock");
        File::create(ready_marker(&workspace, &number)).expect("marker");
        thread::sleep(Duration::from_secs(120));
        return;
    }
    let board = open_board(&workspace, GARDEN);
    let op = writer_op(&role, &setting(NOTE), number.parse().expect("number"));
    File::create(ready_marker(&workspace, &number)).expect("marker");
    wait_for("the start signal", || start_signal(&workspace).exists());
    board.apply(op).expect("applied");
}

fn write_from_threads(workspace: &Path, role: &str, note: &str) {
    let start = Arc::new(Barrier::new(WRITERS));
    let writers: Vec<_> = (0..WRITERS)
        .map(|number| {
            let (workspace, start) = (workspace.to_path_buf(), Arc::clone(&start));
            let op = writer_op(role, note, number);
            thread::spawn(move || {
                let board = open_board(&workspace, GARDEN);
                start.wait();
                board.apply(op).expect("applied");
            })
        })
        .collect();
    for writer in writers {
        writer.join().expect("writer thread");
    }
}

fn write_from_processes(workspace: &Path, role: &str, note: &str) {
    let root = workspace.to_string_lossy();
    let mut writers: Vec<_> = (0..WRITERS)
        .map(|number| {
            spawn_role(
                OTHER_PROCESS,
                role,
                &[
                    (WORKSPACE, root.as_ref()),
                    (NOTE, note),
                    (NUMBER, &number.to_string()),
                ],
            )
        })
        .collect();
    for number in 0..WRITERS {
        wait_for("a writer process", || {
            ready_marker(workspace, &number.to_string()).exists()
        });
    }
    File::create(start_signal(workspace)).expect("start signal");
    for writer in &mut writers {
        assert!(writer.wait().expect("writer process").success());
    }
}

fn assert_every_line_is_kept(workspace: &Path, note: &NoteId, writers: &str) {
    let body = open_board(workspace, GARDEN).note(note).expect("note").body;
    let kept = (0..WRITERS)
        .filter(|number| body.lines().any(|written| written == line(*number)))
        .count();
    assert_eq!(
        kept, WRITERS,
        "{kept} of {WRITERS} lines appended by {writers} are in the note:\n{body}"
    );
}

fn assert_every_section_is_kept(workspace: &Path, writers: &str) {
    let snapshot = open_board(workspace, GARDEN).snapshot().expect("snapshot");
    let kept = (0..WRITERS)
        .filter(|number| {
            snapshot
                .sections
                .iter()
                .any(|section| section.name == section_name(*number))
        })
        .count();
    assert_eq!(
        kept, WRITERS,
        "{kept} of {WRITERS} sections added by {writers} are in the config"
    );
}

#[test]
fn twenty_threads_appending_to_one_note_all_land() {
    let scratch = Scratch::new("append-threads");
    let note = add_note(&scratch.garden(), "Harvest log", "").id;
    write_from_threads(scratch.path(), "append", note.as_str());
    assert_every_line_is_kept(scratch.path(), &note, "threads");
}

#[test]
fn twenty_processes_appending_to_one_note_all_land() {
    let scratch = Scratch::new("append-processes");
    let note = add_note(&scratch.garden(), "Harvest log", "").id;
    write_from_processes(scratch.path(), "append", note.as_str());
    assert_every_line_is_kept(scratch.path(), &note, "processes");
}

#[test]
fn twenty_threads_adding_sections_all_land() {
    let scratch = Scratch::new("sections-threads");
    scratch.garden();
    write_from_threads(scratch.path(), "add-section", "none");
    assert_every_section_is_kept(scratch.path(), "threads");
}

#[test]
fn twenty_processes_adding_sections_all_land() {
    let scratch = Scratch::new("sections-processes");
    scratch.garden();
    write_from_processes(scratch.path(), "add-section", "none");
    assert_every_section_is_kept(scratch.path(), "processes");
}

fn note_text(folder: &Path, id: &NoteId) -> String {
    let names = files_holding(folder, id);
    assert_eq!(names.len(), 1, "files holding the id {id}: {names:?}");
    fs::read_to_string(folder.join(&names[0])).expect("note file")
}

#[test]
fn an_edit_based_on_stale_text_is_a_conflict_and_changes_nothing() {
    let scratch = Scratch::new("stale-body");
    let reader = scratch.garden();
    let seen = add_note(&reader, "Tomato plan", "Six plants along the fence.");
    let folder = scratch.path().join(GARDEN);

    let other_writer = open_board(scratch.path(), GARDEN);
    other_writer
        .apply(set_body(&seen.id, None, "Eight plants, two rows."))
        .expect("outside edit");

    let outcome = reader
        .apply(set_body(
            &seen.id,
            Some(&seen.body),
            "Six plants along the fence, with basil.",
        ))
        .expect("stale edit");
    let on_disk = note_text(&folder, &seen.id);
    assert!(
        on_disk.contains("Eight plants, two rows.") && !on_disk.contains("basil"),
        "the edit based on stale text replaced the other writer's text; the file now reads:\n{on_disk}"
    );
    match outcome {
        Outcome::Conflict { current } => assert_eq!(current.body, "Eight plants, two rows."),
        other => panic!("the edit based on stale text answered {other:?} instead of a conflict"),
    }
}

#[test]
fn a_title_edit_keeps_a_body_that_changed_outside() {
    let scratch = Scratch::new("stale-title");
    let reader = scratch.garden();
    let seen = add_note(&reader, "Tomato plan", "Six plants along the fence.");

    let other_writer = open_board(scratch.path(), GARDEN);
    other_writer
        .apply(set_body(&seen.id, None, "Eight plants, two rows."))
        .expect("outside edit");

    let outcome = reader
        .apply(set_title(&seen.id, Some(&seen.title), "Tomato plan 2027"))
        .expect("title edit");
    match outcome {
        Outcome::Applied { note } => {
            assert_eq!(
                (note.title.as_str(), note.body.as_str()),
                ("Tomato plan 2027", "Eight plants, two rows.")
            );
        }
        other => panic!("the title edit answered {other:?}"),
    }
    let on_disk = note_text(&scratch.path().join(GARDEN), &seen.id);
    assert!(on_disk.contains("# Tomato plan 2027\n\nEight plants, two rows.\n"));
}

#[test]
fn an_edit_of_a_removed_note_answers_not_found_and_brings_nothing_back() {
    let scratch = Scratch::new("removed");
    let reader = scratch.garden();
    let seen = add_note(&reader, "Weed the paths", "Before the rain.");
    let folder = scratch.path().join(GARDEN);
    let name = files_holding(&folder, &seen.id).remove(0);
    reader.snapshot().expect("snapshot");

    let trash = folder.join(TRASH_FOLDER);
    fs::create_dir_all(&trash).expect("trash folder");
    fs::rename(folder.join(&name), trash.join(&name)).expect("move to the trash");
    let trashed = fs::read(trash.join(&name)).expect("trashed file");

    let answer = reader.apply(set_body(&seen.id, Some(&seen.body), "After the rain."));
    let on_board = files_holding(&folder, &seen.id);
    assert!(
        on_board.is_empty(),
        "the removed note is back on the board as {on_board:?}"
    );
    assert_eq!(
        answer,
        Err(Error::NoteNotFound {
            note: seen.id.clone()
        })
    );
    assert_eq!(fs::read(trash.join(&name)).expect("trashed file"), trashed);
}

#[test]
fn an_edit_of_a_renamed_note_lands_in_the_one_renamed_file() {
    let scratch = Scratch::new("renamed");
    let reader = scratch.garden();
    let seen = add_note(&reader, "Tomato plan", "Six plants along the fence.");
    let folder = scratch.path().join(GARDEN);
    reader.snapshot().expect("snapshot");

    let other_writer = open_board(scratch.path(), GARDEN);
    other_writer
        .apply(set_title(&seen.id, None, "Tomato plan for the south fence"))
        .expect("outside rename");

    let outcome = reader
        .apply(set_body(&seen.id, Some(&seen.body), "Eight plants."))
        .expect("body edit");
    let holding = files_holding(&folder, &seen.id);
    assert_eq!(
        holding,
        [format!("tomato-plan-for-the-south-fence-{}.md", seen.id)],
        "{} files hold the id {} after the edit",
        holding.len(),
        seen.id
    );
    match outcome {
        Outcome::Applied { note } => {
            assert_eq!(
                (note.title.as_str(), note.body.as_str()),
                ("Tomato plan for the south fence", "Eight plants.")
            );
        }
        other => panic!("the body edit answered {other:?}"),
    }
}

struct Stalled {
    holder: thread::JoinHandle<Instant>,
}

fn stall_inside_a_config_change(folder: &Path) -> Stalled {
    let (entered, has_entered) = mpsc::channel();
    let folder = folder.to_path_buf();
    let holder = thread::spawn(move || {
        let held = BoardLocks::of_board(&folder)
            .acquire(&[LockKey::Config], LONG_DEADLINE)
            .expect("holder lock");
        let mut config = config::read(&folder).expect("config");
        entered.send(()).expect("signal");
        thread::sleep(STALL);
        let id = SectionId::parse("sholder").expect("section id");
        config
            .add_section(&id, "From the holder", None)
            .expect("section");
        config::write(&folder, &config, &held).expect("config written");
        let left = Instant::now();
        drop(held);
        left
    });
    has_entered.recv().expect("holder inside");
    Stalled { holder }
}

fn section_names(workspace: &Path) -> Vec<String> {
    let snapshot = open_board(workspace, GARDEN).snapshot().expect("snapshot");
    snapshot
        .sections
        .into_iter()
        .map(|section| section.name)
        .collect()
}

#[test]
fn a_writer_waits_for_a_stalled_holder_and_both_changes_are_kept() {
    let scratch = Scratch::new("stalled-wait");
    scratch.garden();
    let started = Instant::now();
    let stalled = stall_inside_a_config_change(&scratch.path().join(GARDEN));

    thread::sleep(ARRIVAL);
    let writer = Workspace::open(scratch.path())
        .expect("workspace")
        .with_lock_deadline(LONG_DEADLINE)
        .board(&GARDEN.parse().expect("board id"))
        .expect("board");
    let answer = writer.apply(add_section("From the writer"));
    let entered = Instant::now();
    let left = stalled.holder.join().expect("holder");

    assert!(
        entered >= left,
        "the writer finished {:.2} s after the start while the holder was inside until {:.2} s",
        entered.duration_since(started).as_secs_f64(),
        left.duration_since(started).as_secs_f64()
    );
    answer.expect("the writer's section");
    assert_eq!(
        section_names(scratch.path()),
        ["From the holder", "From the writer"]
    );
}

#[test]
fn a_writer_with_a_short_deadline_gives_up_and_changes_nothing() {
    let scratch = Scratch::new("stalled-busy");
    scratch.garden();
    let stalled = stall_inside_a_config_change(&scratch.path().join(GARDEN));

    thread::sleep(ARRIVAL);
    let writer = Workspace::open(scratch.path())
        .expect("workspace")
        .with_lock_deadline(SHORT_DEADLINE)
        .board(&GARDEN.parse().expect("board id"))
        .expect("board");
    let answer = writer.apply(add_section("From the writer"));
    stalled.holder.join().expect("holder");

    assert!(
        matches!(answer, Err(Error::Busy { .. })),
        "the writer with a {SHORT_DEADLINE:?} deadline answered {answer:?} while the holder was inside"
    );
    assert_eq!(section_names(scratch.path()), ["From the holder"]);
}

fn leave_a_dead_holder(workspace: &Path, trial: usize) {
    let number = trial.to_string();
    let mut holder = spawn_role(
        OTHER_PROCESS,
        "hold-config",
        &[
            (WORKSPACE, workspace.to_string_lossy().as_ref()),
            (NUMBER, &number),
        ],
    );
    wait_for("the holder process", || {
        ready_marker(workspace, &number).exists()
    });
    holder.kill().expect("kill the holder");
    holder.wait().expect("holder gone");
}

#[test]
fn the_lock_of_a_killed_holder_is_free_at_once() {
    let scratch = Scratch::new("killed-holder");
    let board = scratch.garden();
    leave_a_dead_holder(scratch.path(), 0);

    let started = Instant::now();
    board
        .apply(add_section("After the crash"))
        .expect("section");
    let waited = started.elapsed();
    assert!(
        waited < Duration::from_secs(1),
        "the next writer waited {:.2} s for the lock of a killed holder",
        waited.as_secs_f64()
    );
}

fn age_the_lock_files(folder: &Path) {
    let long_ago = SystemTime::now() - Duration::from_secs(60);
    for entry in fs::read_dir(folder.join(".locks")).expect("locks folder") {
        let path = entry.expect("entry").path();
        File::options()
            .append(true)
            .open(&path)
            .and_then(|file| file.set_modified(long_ago))
            .expect("aged lock file");
    }
}

fn most_holders_at_once(folder: &Path) -> usize {
    let start = Arc::new(Barrier::new(RACERS));
    let inside = Arc::new(AtomicUsize::new(0));
    let most = Arc::new(AtomicUsize::new(0));
    let racers: Vec<_> = (0..RACERS)
        .map(|_| {
            let locks = BoardLocks::of_board(folder);
            let (start, inside, most) =
                (Arc::clone(&start), Arc::clone(&inside), Arc::clone(&most));
            thread::spawn(move || {
                start.wait();
                let held: Result<_, LockError> = locks.acquire(&[LockKey::Config], LONG_DEADLINE);
                let held = held.expect("racer lock");
                most.fetch_max(inside.fetch_add(1, Ordering::SeqCst) + 1, Ordering::SeqCst);
                thread::sleep(Duration::from_micros(200));
                inside.fetch_sub(1, Ordering::SeqCst);
                drop(held);
            })
        })
        .collect();
    for racer in racers {
        racer.join().expect("racer");
    }
    most.load(Ordering::SeqCst)
}

#[test]
fn eight_racers_for_the_lock_of_a_dead_holder_never_hold_it_together() {
    let scratch = Scratch::new("dead-holder-race");
    scratch.garden();
    let folder = scratch.path().join(GARDEN);
    let mut crowded = Vec::new();
    for trial in 0..RACE_TRIALS {
        leave_a_dead_holder(scratch.path(), trial);
        age_the_lock_files(&folder);
        let most = most_holders_at_once(&folder);
        if most > 1 {
            crowded.push(most);
        }
    }
    assert!(
        crowded.is_empty(),
        "{} of {RACE_TRIALS} trials had more than one holder at once, at most {}",
        crowded.len(),
        crowded.iter().max().copied().unwrap_or_default()
    );
}
