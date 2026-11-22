use std::collections::HashMap;

use clap::Subcommand;
use serde::Serialize;
use serde_json::Value;
use wb_core::NoteFile;
use wb_model::{
    BoardConfig, Error, HUE_MAX, HUE_MIN, Invalid, Op, Outcome, SectionId, SectionNode, SectionRef,
    SectionTree, same_section_name,
};

use crate::context::{BoardRef, Context};
use crate::error::{Failure, Result, fail};
use crate::notes;
use crate::output::{emit, or_dash, table};

const LAYOUT_KEY: &str = "layout";
const NEXT_KEY: &str = "next";

#[derive(Subcommand)]
pub enum SectionCommand {
    #[command(
        about = "List the sections of the board as a tree, with their notes counted",
        long_about = "List the sections of the board as a tree, with the number of notes in each (not counting sections inside it), and which is the default and which the archive section.\n\nJSON: [{id, name, path, parent, depth, note, hue, notes, listed, default, archive, layout, next}]; `listed` is false for a section drawn in the app that board.config.json does not list yet; `layout` is the section's layout or null; `next` the id of the section its notes move on to, or null."
    )]
    List,
    #[command(
        about = "Add a section. The app draws its box",
        long_about = "Add a section to the board's config. The app draws its box, inside its parent's. Sibling sections may not share a name.\n\nJSON: {id, name, path, parent}",
        after_long_help = "Examples:\n  wb --workspace boards --board garden section add Beds --hue 120\n  wb --workspace boards --board garden section add Spring --parent Beds --note \"Started indoors\""
    )]
    Add {
        #[arg(help = "The section's name")]
        name: String,
        #[arg(
            long,
            value_name = "SECTION",
            help = "The section to put it in: a path like `Beds / Spring`, or an id"
        )]
        parent: Option<String>,
        #[arg(long, help = "A line shown under the section's name")]
        note: Option<String>,
        #[arg(
            long,
            help = "Its colour, as a hue from 0 to 360 [default: the app picks one]"
        )]
        hue: Option<f64>,
    },
}

pub fn run(cx: &Context, command: SectionCommand) -> Result<()> {
    match command {
        SectionCommand::List => emit(cx.json, &list(cx)?[..], render_list),
        SectionCommand::Add {
            name,
            parent,
            note,
            hue,
        } => {
            let request = NewSection {
                name,
                parent,
                note: note.unwrap_or_default(),
                hue,
            };
            emit(cx.json, &add(cx, request)?, render_added)
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SectionRow {
    pub id: String,
    pub name: String,
    pub path: String,
    pub parent: Option<String>,
    pub depth: usize,
    pub note: String,
    pub hue: Option<f64>,
    pub notes: usize,
    pub listed: bool,
    pub default: bool,
    pub archive: bool,
    pub layout: Option<Value>,
    pub next: Option<String>,
}

pub fn path_text(sections: &SectionTree, id: &SectionId) -> String {
    sections
        .path_of(id)
        .map(|path| path.to_string())
        .unwrap_or_default()
}

pub fn counts(sections: &SectionTree, notes: &[NoteFile]) -> (HashMap<SectionId, usize>, usize) {
    let (mut counts, mut unsectioned) = (HashMap::new(), 0);
    for file in notes.iter().filter(|file| !notes::is_rules(file)) {
        match &file.note.placement.section {
            None => unsectioned += 1,
            Some(path) => {
                if let Some(id) = sections.resolve(path) {
                    *counts.entry(id.clone()).or_insert(0) += 1;
                }
            }
        }
    }
    (counts, unsectioned)
}

pub fn rows(config: &BoardConfig, notes: &[NoteFile]) -> Vec<SectionRow> {
    let sections = SectionTree::from_config(config);
    let (counts, _) = counts(&sections, notes);
    let mut rows = Vec::new();
    let mut add = |node: &SectionNode, parent: Option<&SectionId>, depth: usize| {
        let entry = config
            .section_entries()
            .iter()
            .find(|entry| entry.id == node.id.as_str());
        let extra = |key: &str| entry.and_then(|entry| entry.extra.get(key));
        rows.push(SectionRow {
            id: node.id.to_string(),
            name: node.name.clone(),
            path: path_text(&sections, &node.id),
            parent: parent.map(ToString::to_string),
            depth,
            note: entry.map(|entry| entry.note.clone()).unwrap_or_default(),
            hue: node.hue,
            notes: counts.get(&node.id).copied().unwrap_or(0),
            listed: true,
            default: config.default_section.as_deref() == Some(node.id.as_str()),
            archive: config.archive_section.as_deref() == Some(node.id.as_str()),
            layout: extra(LAYOUT_KEY)
                .filter(|layout| !layout.is_null())
                .cloned(),
            next: extra(NEXT_KEY).and_then(Value::as_str).map(str::to_owned),
        });
    };
    let mut stack: Vec<(&SectionNode, Option<&SectionId>, usize)> = Vec::new();
    let top = sections.nodes();
    stack.extend(top.iter().rev().map(|node| (node, None, 0)));
    while let Some((node, parent, depth)) = stack.pop() {
        add(node, parent, depth);
        stack.extend(
            node.children
                .iter()
                .rev()
                .map(|child| (child, Some(&node.id), depth + 1)),
        );
    }
    rows
}

pub fn layout_text(layout: Option<&Value>) -> String {
    layout.map_or_else(|| or_dash(None), Value::to_string)
}

fn list(cx: &Context) -> Result<Vec<SectionRow>> {
    let board = cx.board()?;
    let notes = board.board.notes()?;
    Ok(rows(&board.config()?, &notes))
}

pub fn render_tree(rows: &[SectionRow]) -> String {
    let role = |row: &SectionRow| match (row.default, row.archive) {
        (true, true) => "default, archive",
        (true, false) => "default",
        (false, true) => "archive",
        (false, false) => "",
    };
    let path = |id: &str| {
        rows.iter()
            .find(|row| row.id == id)
            .map_or_else(|| format!("{id} (missing)"), |row| row.path.clone())
    };
    let lines: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            vec![
                format!("{}{}", "  ".repeat(row.depth), row.name),
                row.notes.to_string(),
                row.id.clone(),
                role(row).to_owned(),
                layout_text(row.layout.as_ref()),
                row.next.as_deref().map_or_else(|| or_dash(None), path),
            ]
        })
        .collect();
    table(
        &["SECTION", "NOTES", "ID", "ROLE", "LAYOUT", "NEXT"],
        &lines,
    )
}

fn render_list(rows: &[SectionRow]) -> String {
    if rows.is_empty() {
        "No sections.".to_owned()
    } else {
        render_tree(rows)
    }
}

pub fn missing_section(text: &str, sections: &SectionTree) -> Failure {
    let paths: Vec<String> = rows_in_order(sections);
    let known = if paths.is_empty() {
        "the board has no sections yet".to_owned()
    } else {
        format!("sections: {}", paths.join("; "))
    };
    Failure(format!(
        "there is no section \"{}\" on this board; {known}",
        text.trim()
    ))
}

fn rows_in_order(sections: &SectionTree) -> Vec<String> {
    let mut paths = Vec::new();
    let mut stack: Vec<SectionNode> = sections.nodes().into_iter().rev().collect();
    while let Some(node) = stack.pop() {
        paths.push(path_text(sections, &node.id));
        stack.extend(node.children.into_iter().rev());
    }
    paths
}

pub fn named(sections: &SectionTree, text: &str) -> Result<SectionRef> {
    match sections.lookup(text) {
        Some(id) => Ok(SectionRef::Id { id: id.clone() }),
        None => Err(missing_section(text, sections)),
    }
}

#[derive(Debug, Serialize)]
pub struct AddedSection {
    pub id: String,
    pub name: String,
    pub path: String,
    pub parent: Option<String>,
}

pub struct NewSection {
    pub name: String,
    pub parent: Option<String>,
    pub note: String,
    pub hue: Option<f64>,
}

fn add(cx: &Context, request: NewSection) -> Result<AddedSection> {
    let board = cx.board()?;
    let parent = match request.parent.as_deref() {
        Some(text) => Some(named(&board.sections()?, text)?),
        None => None,
    };
    let parent_id = match &parent {
        Some(SectionRef::Id { id }) => Some(id.clone()),
        _ => None,
    };
    let op = Op::AddSection {
        name: request.name,
        parent,
        note: request.note,
        hue: request.hue,
    };
    let outcome = board.board.apply(op).map_err(|error| match error {
        Error::Invalid {
            problem: Invalid::SectionNameTaken { name },
        } => name_taken(&board, &name, parent_id.as_ref()),
        Error::Invalid {
            problem: Invalid::HueOutOfRange,
        } => Failure(format!(
            "a hue is a number from {HUE_MIN} to {HUE_MAX}, not {}",
            request.hue.unwrap_or_default()
        )),
        other => Failure::from(other),
    })?;
    let Outcome::SectionAdded { section } = outcome else {
        fail!("the section was written but cannot be read back")
    };
    let sections = board.sections()?;
    Ok(AddedSection {
        path: path_text(&sections, &section.id),
        id: section.id.to_string(),
        name: section.name,
        parent: parent_id.map(|id| id.to_string()),
    })
}

fn name_taken(board: &BoardRef, name: &str, parent: Option<&SectionId>) -> Failure {
    let Ok(config) = board.config() else {
        return Failure(format!("there is already a section \"{name}\" there"));
    };
    let sections = SectionTree::from_config(&config);
    let sibling = config.section_entries().iter().find(|entry| {
        entry.parent.as_deref() == parent.map(SectionId::as_str)
            && same_section_name(&entry.name, name)
    });
    let place = match parent {
        Some(parent) => format!("in \"{}\"", path_text(&sections, parent)),
        None => "at the top level".to_owned(),
    };
    match sibling {
        Some(sibling) => Failure(format!(
            "there is already a section \"{}\" {place} ({}); sibling sections need different names",
            sibling.name, sibling.id
        )),
        None => Failure(format!(
            "there is already a section \"{name}\" {place}; sibling sections need different names"
        )),
    }
}

fn render_added(added: &AddedSection) -> String {
    format!("{}\n{}", added.id, added.path)
}
