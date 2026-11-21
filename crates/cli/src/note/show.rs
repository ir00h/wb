use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;
use wb_model::NoteKind;

use crate::context::Context;
use crate::error::Result;
use crate::notes::{section_of, tag_names};
use crate::output::{list_or_dash, or_dash};

use super::{live, note_ref};

const FIELD_WIDTH: usize = 9;

#[derive(Debug, Serialize)]
pub struct Shown {
    pub version: Option<String>,
    pub head: Option<String>,
    pub created: Option<f64>,
    pub stack: Option<String>,
    pub order: Option<f64>,
    pub id: String,
    pub board: String,
    pub path: String,
    pub title: String,
    pub body: String,
    pub section: Option<String>,
    pub tags: Vec<String>,
    pub source: String,
    pub kind: Option<&'static str>,
    pub opens: Option<String>,
    pub extra: BTreeMap<String, String>,
    pub links: Vec<Value>,
    pub backlinks: Vec<Value>,
    pub attachments: Vec<Value>,
}

fn kind_name(kind: &NoteKind) -> Option<&'static str> {
    match kind {
        NoteKind::Board => Some("board"),
        NoteKind::Image => Some("image"),
        NoteKind::Rules => Some("rules"),
        NoteKind::Plain | NoteKind::Other(_) => None,
    }
}

pub fn show(cx: &Context, text: &str) -> Result<Shown> {
    let (board, id) = note_ref(cx, text)?;
    let file = live(&board, &id)?;
    let note = file.note;
    Ok(Shown {
        version: None,
        head: None,
        created: None,
        stack: None,
        order: None,
        id: note.id.to_string(),
        board: board.id.to_string(),
        path: file.path.display().to_string(),
        section: section_of(&note),
        tags: tag_names(&note),
        kind: kind_name(&note.kind),
        title: note.title,
        body: note.body,
        source: note.source,
        opens: note.board,
        extra: note.extra.into_iter().collect(),
        links: Vec::new(),
        backlinks: Vec::new(),
        attachments: Vec::new(),
    })
}

pub fn render(note: &Shown) -> String {
    let title = if note.title.is_empty() {
        "(no title)"
    } else {
        &note.title
    };
    let mut out = vec![format!("{}  {title}", note.id)];
    let mut field = |key: &str, value: &str| out.push(format!("{key:<FIELD_WIDTH$} {value}"));
    field("board:", &note.board);
    field("file:", &note.path);
    field("section:", &or_dash(note.section.as_deref()));
    field("tags:", &list_or_dash(&note.tags));
    if !note.source.is_empty() {
        field("source:", &note.source);
    }
    if let Some(kind) = note.kind {
        field("kind:", kind);
    }
    if let Some(opens) = &note.opens {
        field("opens:", opens);
    }
    for (key, value) in &note.extra {
        out.push(format!("{key}: {value}"));
    }
    if !note.body.is_empty() {
        out.push(String::new());
        out.push(note.body.clone());
    }
    out.push(String::new());
    out.push("Backlinks: none".to_owned());
    out.join("\n")
}
