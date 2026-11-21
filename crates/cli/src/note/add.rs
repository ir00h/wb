use serde::Serialize;
use wb_model::{DraftSection, NoteDraft, Op, Outcome, SectionId, SectionTree};

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
    let Some(text) = named.map(str::trim) else {
        return Ok(DraftSection::Default);
    };
    if !text.eq_ignore_ascii_case(NO_SECTION) {
        let section = section::named(&board.sections()?, text)?;
        return Ok(DraftSection::Named { section });
    }
    let config = board.config()?;
    let sections = SectionTree::from_config(&config);
    let default = config
        .default_section
        .as_deref()
        .and_then(|id| SectionId::parse(id).ok())
        .filter(|id| sections.contains(id));
    if let Some(default) = default {
        fail!(
            "this board puts a note without a section into its default section \"{}\", so --section none cannot be kept yet; name a section instead",
            section::path_text(&sections, &default)
        )
    }
    Ok(DraftSection::NoSection)
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
        source: String::new(),
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
