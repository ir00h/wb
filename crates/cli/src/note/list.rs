use clap::Args;
use serde::Serialize;
use wb_core::NoteFile;
use wb_model::{SectionPath, Tag, same_section_name};

use crate::context::Context;
use crate::error::Result;
use crate::notes::{self, label, malformed_tag, section_of, tag_names};
use crate::output::{list_or_dash, or_dash, table};
use crate::section::missing_section;

#[derive(Args)]
pub struct FilterArgs {
    #[arg(
        long,
        value_name = "SECTION",
        help = "Only notes in this section (a path like `Beds / Spring`, or an id) and the sections inside it"
    )]
    section: Option<String>,
    #[arg(
        long,
        requires = "section",
        help = "With --section: leave out the sections inside it"
    )]
    exact: bool,
    #[arg(
        long = "tag",
        value_name = "TAG",
        help = "Only notes with this tag. Repeat it for notes that carry every one"
    )]
    tags: Vec<String>,
    #[arg(
        long,
        help = "Also the board's rules note, which is left out otherwise"
    )]
    rules: bool,
}

#[derive(Debug, Serialize)]
pub struct NoteSummary {
    pub id: String,
    pub title: String,
    pub section: Option<String>,
    pub stack: Option<String>,
    pub order: Option<f64>,
    pub tags: Vec<String>,
    pub path: String,
}

fn within(note: Option<&SectionPath>, wanted: &SectionPath, exact: bool) -> bool {
    let names = note.map(SectionPath::names).unwrap_or_default();
    let wanted = wanted.names();
    names.len() >= wanted.len()
        && names
            .iter()
            .zip(wanted)
            .all(|(name, wanted)| same_section_name(name, wanted))
        && (!exact || names.len() == wanted.len())
}

pub fn list(cx: &Context, filter: &FilterArgs) -> Result<Vec<NoteSummary>> {
    let board = cx.board()?;
    let wanted = match &filter.section {
        Some(text) => {
            let sections = board.sections()?;
            let Some(path) = sections.lookup(text).and_then(|id| sections.path_of(id)) else {
                return Err(missing_section(text, &sections));
            };
            Some(path)
        }
        None => None,
    };
    let tags = Tag::list(&filter.tags);
    if let Some(tag) = tags.iter().find(|tag| !tag.is_well_formed()) {
        return Err(malformed_tag(&filter.tags, tag.as_str()));
    }
    let mut files: Vec<NoteFile> = board
        .board
        .notes()?
        .into_iter()
        .filter(|file| filter.rules || !notes::is_rules(file))
        .filter(|file| tags.iter().all(|tag| file.note.tags.contains(tag)))
        .filter(|file| {
            wanted.as_ref().is_none_or(|wanted| {
                within(file.note.placement.section.as_ref(), wanted, filter.exact)
            })
        })
        .collect();
    let key = |file: &NoteFile| {
        (
            section_of(&file.note).unwrap_or_default().to_lowercase(),
            label(&file.note).to_lowercase(),
            file.note.id.to_string(),
        )
    };
    files.sort_by_cached_key(key);
    Ok(files
        .iter()
        .map(|file| NoteSummary {
            id: file.note.id.to_string(),
            title: label(&file.note),
            section: section_of(&file.note),
            stack: None,
            order: None,
            tags: tag_names(&file.note),
            path: file.path.display().to_string(),
        })
        .collect())
}

pub fn render(notes: &[NoteSummary]) -> String {
    if notes.is_empty() {
        return "No notes.".to_owned();
    }
    let rows: Vec<Vec<String>> = notes
        .iter()
        .map(|note| {
            vec![
                note.id.clone(),
                note.title.clone(),
                or_dash(note.section.as_deref()),
                list_or_dash(&note.tags),
            ]
        })
        .collect();
    table(&["ID", "TITLE", "SECTION", "TAGS"], &rows)
}
