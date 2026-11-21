use std::hash::{BuildHasher, RandomState};

use wb_model::{NoteId, SectionId};

const SUFFIX_LENGTH: usize = 6;
const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
const NOTE_PREFIX: char = 'n';
const SECTION_PREFIX: char = 's';

fn random_suffix() -> String {
    let mut bits = RandomState::new().hash_one(std::process::id());
    let base = ALPHABET.len() as u64;
    let mut suffix = String::with_capacity(SUFFIX_LENGTH);
    for _ in 0..SUFFIX_LENGTH {
        let digit = usize::try_from(bits % base).unwrap_or_default();
        suffix.push(char::from(ALPHABET[digit]));
        bits /= base;
    }
    suffix
}

fn fresh<Id>(
    prefix: char,
    parse: impl Fn(&str) -> Option<Id>,
    is_taken: impl Fn(&Id) -> bool,
) -> Id {
    loop {
        if let Some(id) = parse(&format!("{prefix}{}", random_suffix()))
            && !is_taken(&id)
        {
            return id;
        }
    }
}

pub fn new_note_id(is_taken: impl Fn(&NoteId) -> bool) -> NoteId {
    fresh(NOTE_PREFIX, |text| NoteId::parse(text).ok(), is_taken)
}

pub fn new_section_id(is_taken: impl Fn(&SectionId) -> bool) -> SectionId {
    fresh(SECTION_PREFIX, |text| SectionId::parse(text).ok(), is_taken)
}
