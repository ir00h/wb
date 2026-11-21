use alloc::string::String;

use crate::note::{Note, clean_body, clean_title};
use crate::text;
use crate::wire::{BodyChange, Change, NoteEdit};

const PARAGRAPH_BREAK: &str = "\n\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaleBase {
    Title,
    Body,
}

pub fn edit_note(note: &Note, edit: &NoteEdit) -> Result<Note, StaleBase> {
    let mut edited = note.clone();
    if let Some(title) = &edit.title {
        if is_stale(title, &note.title, clean_title) {
            return Err(StaleBase::Title);
        }
        edited.title = clean_title(&title.to);
    }
    match &edit.body {
        Some(BodyChange::Set(body)) => {
            if is_stale(body, &note.body, clean_body) {
                return Err(StaleBase::Body);
            }
            edited.body = clean_body(&body.to);
        }
        Some(BodyChange::Append { text }) => edited.body = appended(&note.body, text),
        None => {}
    }
    Ok(edited)
}

fn is_stale(change: &Change<String>, current: &str, clean: fn(&str) -> String) -> bool {
    change
        .base
        .as_deref()
        .is_some_and(|base| clean(base) != clean(current))
}

fn appended(body: &str, more: &str) -> String {
    let (body, more) = (clean_body(body), text::trim(more));
    if body.is_empty() || more.is_empty() {
        return body + more;
    }
    body + PARAGRAPH_BREAK + more
}
