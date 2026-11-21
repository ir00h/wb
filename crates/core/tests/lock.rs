mod common;

use std::fs;
use std::time::Duration;

use common::Scratch;
use wb_core::lock::{BoardLocks, LockError, LockKey, replace_file};
use wb_model::NoteId;

const AT_ONCE: Duration = Duration::from_millis(50);

fn note(id: &str) -> LockKey {
    LockKey::Note(NoteId::parse(id).expect("note id"))
}

#[test]
fn a_second_taker_is_busy_until_the_first_lets_go() {
    let scratch = Scratch::new("lock-busy");
    let locks = BoardLocks::of_board(scratch.path());
    let held = locks.acquire(&[note("n1")], AT_ONCE).expect("first");
    assert!(held.covers(&note("n1")) && !held.covers(&LockKey::Config));

    let second = BoardLocks::of_board(scratch.path()).acquire(&[note("n1")], AT_ONCE);
    assert!(
        matches!(&second, Err(LockError::Busy { key, waited }) if *key == note("n1") && *waited >= AT_ONCE),
        "{second:?}"
    );
    locks.acquire(&[note("n2")], AT_ONCE).expect("another note");

    drop(held);
    locks.acquire(&[note("n1")], AT_ONCE).expect("free again");
}

#[test]
fn several_keys_are_taken_in_one_call_whatever_their_order() {
    let scratch = Scratch::new("lock-many");
    let locks = BoardLocks::of_board(scratch.path());
    let keys = [note("n2"), LockKey::Config, note("n1"), note("n2")];
    let held = locks.acquire(&keys, AT_ONCE).expect("all keys");
    assert!(keys.iter().all(|key| held.covers(key)));
    for key in [note("n1"), note("n2"), LockKey::Config] {
        assert!(matches!(
            locks.acquire(std::slice::from_ref(&key), AT_ONCE),
            Err(LockError::Busy { .. })
        ));
    }
}

#[test]
fn a_file_is_replaced_whole_and_nothing_is_left_beside_it() {
    let scratch = Scratch::new("replace");
    let path = scratch.path().join("plant-list.md");
    replace_file(&path, b"first").expect("written");
    replace_file(&path, b"second, longer").expect("replaced");
    assert_eq!(fs::read(&path).expect("file"), b"second, longer");
    let names: Vec<_> = fs::read_dir(scratch.path())
        .expect("folder")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    assert_eq!(names, ["plant-list.md"]);
}
