use serde::Serialize;
use wb_model::{BodyChange, Change, Error, NoteEdit, Op, Outcome};

use crate::context::Context;
use crate::error::{Failure, Result, fail};

use super::{not_found, note_ref};

#[derive(Debug, Serialize)]
pub struct Edited {
    pub id: String,
    pub path: String,
    pub title: String,
}

fn blind(to: String) -> Change<String> {
    Change { base: None, to }
}

pub fn edit(
    cx: &Context,
    text: &str,
    title: Option<String>,
    body: Option<String>,
    append: bool,
) -> Result<Edited> {
    let (board, id) = note_ref(cx, text)?;
    if title.is_none() && body.is_none() {
        fail!("nothing to change: pass --title, or text with --body, --file or on stdin")
    }
    let Ok(note) = id.parse() else {
        return Err(not_found(&board, &id));
    };
    let edit = NoteEdit {
        title: title.map(blind),
        body: body.map(|text| {
            if append {
                BodyChange::Append { text }
            } else {
                BodyChange::Set(blind(text))
            }
        }),
    };
    let outcome = board
        .board
        .apply(Op::EditNote { note, edit })
        .map_err(|error| match error {
            Error::NoteNotFound { .. } => not_found(&board, &id),
            other => Failure::from(other),
        })?;
    let Outcome::Applied { note } = outcome else {
        fail!("the note {id} changed while it was being edited; nothing was written")
    };
    let file = board.board.note_file(&note.id)?;
    Ok(Edited {
        id: file.note.id.to_string(),
        path: file.path.display().to_string(),
        title: file.note.title,
    })
}

pub fn render(edited: &Edited) -> String {
    format!("Changed {}: {}", edited.id, edited.path)
}
