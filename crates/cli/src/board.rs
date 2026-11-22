use std::collections::BTreeMap;
use std::fs;

use clap::Subcommand;
use serde::Serialize;
use serde_json::Value;
use wb_model::{SectionId, SectionTree, text};

use crate::context::Context;
use crate::error::{Failure, Result, fail};
use crate::notes;
use crate::output::{count, emit, list_or_dash, or_dash, table};
use crate::section::{self, SectionRow};

#[derive(Subcommand)]
pub enum BoardCommand {
    #[command(
        about = "List every board in the workspace",
        long_about = "List every board in the workspace, parents before the boards inside them, with the number of notes, the board's colour and the linked repositories.\n\nJSON: [{id, name, notes, repos, hue}]; `hue` is null for a board without a colour."
    )]
    List,
    #[command(
        about = "Show a board: its sections as a tree, its tags and its settings",
        long_about = "Show a board: its name, its sections as a tree with the number of notes in each, the tags in use with their counts, the default and archive section, the automatic tags, the linked repositories, the board's colour and whether it has rules. The rules note is not counted in a section, nor as a note without one.\n\nJSON: {id, name, path, notes, unsectioned, sections: [{id, name, path, parent, depth, note, hue, notes, listed, default, archive, layout, next}], tags: [{name, notes}], defaultSection, archiveSection, autoTags, repos, hue, layout, rules: {id, title, path} | null}; `layout` is null where none is set."
    )]
    Show {
        #[arg(help = "The board [default: the current board]")]
        id: Option<String>,
    },
    #[command(
        about = "Create a board, at the top level or inside another board",
        long_about = "Create a board, at the top level or inside another board.\n\nJSON: {id, name, path, card: {board, id, path} | null}",
        after_long_help = "Examples:\n  wb --workspace boards board add Garden\n  wb --workspace boards board add Recipes --parent kitchen"
    )]
    Add {
        #[arg(help = "The board's name. Its folder is made from it")]
        name: String,
        #[arg(long, value_name = "BOARD", help = "The board to create it inside")]
        parent: Option<String>,
    },
}

pub fn run(cx: &Context, command: BoardCommand) -> Result<()> {
    match command {
        BoardCommand::List => emit(cx.json, &list(cx)?[..], render_list),
        BoardCommand::Show { id } => emit(cx.json, &show(cx, id.as_deref())?, render_show),
        BoardCommand::Add { name, parent } => {
            emit(cx.json, &add(cx, &name, parent.as_deref())?, render_added)
        }
    }
}

#[derive(Debug, Serialize)]
pub struct BoardRow {
    pub id: String,
    pub name: String,
    pub notes: usize,
    pub repos: Vec<String>,
    pub hue: Option<f64>,
}

fn list(cx: &Context) -> Result<Vec<BoardRow>> {
    let workspace = cx.workspace()?;
    workspace
        .boards()?
        .into_iter()
        .map(|info| {
            let board = cx.board_named(info.id.as_str())?;
            let config = board.board.config().unwrap_or_default();
            Ok(BoardRow {
                notes: board.board.notes()?.len(),
                id: info.id.to_string(),
                name: info.name,
                repos: config.repos.unwrap_or_default(),
                hue: config.hue.as_ref().and_then(serde_json::Number::as_f64),
            })
        })
        .collect()
}

fn hue_text(hue: Option<f64>) -> String {
    hue.map_or_else(|| or_dash(None), |hue| hue.to_string())
}

fn render_list(rows: &[BoardRow]) -> String {
    if rows.is_empty() {
        return "No boards.".to_owned();
    }
    let lines: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            vec![
                row.id.clone(),
                row.name.clone(),
                row.notes.to_string(),
                hue_text(row.hue),
                list_or_dash(&row.repos),
            ]
        })
        .collect();
    table(&["ID", "NAME", "NOTES", "HUE", "REPOS"], &lines)
}

#[derive(Debug, Serialize)]
pub struct TagCount {
    pub name: String,
    pub notes: usize,
}

#[derive(Debug, Serialize)]
pub struct RulesRef {
    pub id: String,
    pub title: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShownBoard {
    pub id: String,
    pub name: String,
    pub path: String,
    pub notes: usize,
    pub unsectioned: usize,
    pub sections: Vec<SectionRow>,
    pub tags: Vec<TagCount>,
    pub default_section: Option<String>,
    pub archive_section: Option<String>,
    pub auto_tags: Vec<String>,
    pub repos: Vec<String>,
    pub hue: Option<f64>,
    pub layout: Option<Value>,
    pub rules: Option<RulesRef>,
}

fn show(cx: &Context, id: Option<&str>) -> Result<ShownBoard> {
    let board = match id {
        Some(id) => cx.board_named(id)?,
        None => cx.board()?,
    };
    let files = board.board.notes()?;
    let config = board.board.config()?;
    let sections = SectionTree::from_config(&config);
    let (_, unsectioned) = section::counts(&sections, &files);
    let mut tally: BTreeMap<String, usize> = BTreeMap::new();
    for tag in files.iter().flat_map(|file| &file.note.tags) {
        *tally.entry(tag.to_string()).or_insert(0) += 1;
    }
    let mut tags: Vec<TagCount> = tally
        .into_iter()
        .map(|(name, notes)| TagCount { name, notes })
        .collect();
    tags.sort_by(|one, other| {
        other
            .notes
            .cmp(&one.notes)
            .then_with(|| one.name.cmp(&other.name))
    });
    let section_path = |id: Option<&String>| {
        id.map(|text| match SectionId::parse(text) {
            Ok(id) if sections.contains(&id) => section::path_text(&sections, &id),
            _ => section::missing(text),
        })
    };
    let rules = files
        .iter()
        .find(|file| notes::is_rules(file))
        .map(|file| RulesRef {
            id: file.note.id.to_string(),
            title: file.note.title.clone(),
            path: file.path.display().to_string(),
        });
    Ok(ShownBoard {
        id: board.id.to_string(),
        name: board.board.info()?.name,
        path: board.board.folder().display().to_string(),
        notes: files.len(),
        unsectioned,
        sections: section::rows(&config, &files),
        tags,
        default_section: section_path(config.default_section.as_ref()),
        archive_section: section_path(config.archive_section.as_ref()),
        auto_tags: config.auto_tags.clone().unwrap_or_default(),
        repos: config.repos.clone().unwrap_or_default(),
        hue: config.hue.as_ref().and_then(serde_json::Number::as_f64),
        layout: config.layout.clone().filter(|layout| !layout.is_null()),
        rules,
    })
}

fn render_show(board: &ShownBoard) -> String {
    let tags: Vec<String> = board
        .tags
        .iter()
        .map(|tag| format!("{} ({})", tag.name, tag.notes))
        .collect();
    let sections = if board.sections.is_empty() {
        "No sections.".to_owned()
    } else {
        section::render_tree(&board.sections)
    };
    let rules = match &board.rules {
        Some(rules) => format!(
            "Rules: {} ({}), read them with `wb rules`",
            rules.id, rules.path
        ),
        None => "Rules: none; `wb rules init` writes a first version".to_owned(),
    };
    [
        format!("{} ({})", board.name, board.id),
        format!(
            "{}, {} in no section",
            count(board.notes, "note", "notes"),
            board.unsectioned
        ),
        board.path.clone(),
        String::new(),
        sections,
        String::new(),
        format!("Tags: {}", list_or_dash(&tags)),
        format!(
            "Default section: {}",
            or_dash(board.default_section.as_deref())
        ),
        format!(
            "Archive section: {}",
            or_dash(board.archive_section.as_deref())
        ),
        format!("Automatic tags: {}", list_or_dash(&board.auto_tags)),
        format!("Repositories: {}", list_or_dash(&board.repos)),
        format!(
            "Colour: {}",
            board
                .hue
                .map_or_else(|| or_dash(None), |hue| format!("hue {hue}"))
        ),
        format!("Layout: {}", section::layout_text(board.layout.as_ref())),
        rules,
    ]
    .join("\n")
}

#[derive(Debug, Serialize)]
pub struct AddedBoard {
    pub id: String,
    pub name: String,
    pub path: String,
    pub card: Option<Value>,
}

fn add(cx: &Context, name: &str, parent: Option<&str>) -> Result<AddedBoard> {
    if text::single_spaced(name).is_empty() {
        fail!("a board needs a name")
    }
    let parent = parent.map(|parent| cx.board_named(parent)).transpose()?;
    fs::create_dir_all(&cx.root).map_err(|error| {
        Failure(format!(
            "could not make the workspace {}: {error}",
            cx.root.display()
        ))
    })?;
    let workspace = cx.workspace()?;
    let added = workspace.add_board(name, parent.as_ref().map(|parent| &parent.id))?;
    let made = cx.board_named(added.id.as_str())?;
    Ok(AddedBoard {
        id: added.id.to_string(),
        name: added.name,
        path: made.board.folder().display().to_string(),
        card: None,
    })
}

fn render_added(added: &AddedBoard) -> String {
    format!("{}\n{}", added.id, added.path)
}
