use serde::Serialize;
use wb_model::{DraftSection, NoteDraft, Op, Outcome};

use crate::context::{BoardRef, Context};
use crate::error::{Result, fail};
use crate::notes::{section_of, tag_names};
use crate::section;

const NO_SECTION: &str = "none";
const HEADING: &str = "# ";

pub struct NewNote {
    pub title: Option<String>,
    pub section: Option<String>,
    pub tags: Vec<String>,
    pub source: String,
    pub body: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Added {
    pub id: String,
    pub path: String,
    pub board: String,
    pub section: Option<String>,
    pub stack: Option<String>,
    pub order: Option<f64>,
    pub tags: Vec<String>,
}

fn title_from_body(body: &str) -> Option<(String, String)> {
    let body = body.trim_start_matches(['\n', '\r']);
    let (first, rest) = body.split_once('\n').unwrap_or((body, ""));
    let title = first.strip_prefix(HEADING)?.trim();
    (!title.is_empty()).then(|| (title.to_owned(), rest.to_owned()))
}

fn draft_section(board: &BoardRef, named: Option<&str>) -> Result<DraftSection> {
    match named.map(str::trim) {
        None => Ok(DraftSection::Default),
        Some(text) if text.eq_ignore_ascii_case(NO_SECTION) => Ok(DraftSection::NoSection),
        Some(text) => Ok(DraftSection::Named {
            section: section::named(&board.sections()?, text)?,
        }),
    }
}

pub fn add(cx: &Context, request: NewNote) -> Result<Added> {
    let board = cx.board()?;
    let mut body = request.body.unwrap_or_default();
    let title = match request.title {
        Some(title) => title,
        None => match title_from_body(&body) {
            Some((title, rest)) => {
                body = rest;
                title
            }
            None => String::new(),
        },
    };
    let draft = NoteDraft {
        title,
        body,
        section: draft_section(&board, request.section.as_deref())?,
        tags: request.tags,
        source: request.source,
    };
    let Outcome::Applied { note } = board.board.apply(Op::AddNote { draft })? else {
        fail!("the note was written but cannot be read back")
    };
    let file = board.board.note_file(&note.id)?;
    Ok(Added {
        id: file.note.id.to_string(),
        path: file.path.display().to_string(),
        board: board.id.to_string(),
        section: section_of(&file.note),
        stack: None,
        order: None,
        tags: tag_names(&file.note),
    })
}

pub fn render(added: &Added) -> String {
    format!("{}\n{}", added.id, added.path)
}
