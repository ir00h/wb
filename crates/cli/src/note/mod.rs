mod add;
mod edit;
mod list;
mod show;

use std::fs;
use std::io::{IsTerminal, Read};
use std::path::PathBuf;

use clap::{Args, Subcommand};
use wb_core::NoteFile;
use wb_model::{Error, NoteId};

use crate::context::{BoardRef, Context};
use crate::error::{Failure, Result};
use crate::output::emit;

const STDIN_PATH: &str = "-";

#[derive(Subcommand)]
pub enum NoteCommand {
    #[command(
        about = "List the notes of the board",
        long_about = "List the notes of the board, sorted by section and title. The board's rules note is left out unless --rules.\n\nJSON: [{id, title, section, stack, order, tags, path}]; `stack` and `order` are null for a note in no stack or without a place given by hand.",
        after_long_help = "Examples:\n  wb --workspace boards --board garden note list --section Beds\n  wb --workspace boards --board garden note list --tag plan --tag watering\n  wb --workspace boards --board garden note list --section Beds --exact --json"
    )]
    List {
        #[command(flatten)]
        filter: list::FilterArgs,
    },
    #[command(
        about = "Show a note: its front matter, body, backlinks and attachments",
        disable_version_flag = true,
        long_about = "Show a note: its front matter, its body, what it links to, what links to it from its board, and the files attached to it.\n\nJSON: {version, head, created, stack, order, id, board, path, title, body, section, tags, source, kind, opens, extra: {key: value}, links: [{target, board, id, label, exists, title}], backlinks: [{board, id, title, section, path, label}], attachments: [{name, link, path, folder}]}; `version` is the one shown, `created` when the note was made (ms since 1970)."
    )]
    Show {
        #[arg(help = "The note: `id`, or `board#id`")]
        id: String,
    },
    #[command(
        about = "Add a note. The body comes from --body, --file or stdin",
        long_about = "Add a note. The body comes from --body, --file (`-` for stdin) or stdin when it is not a terminal. Without --title, a body that starts with a `# Heading` line gives the title. A note without --section goes into the board's default section.\n\nRefused: a section the config lacks, a tag that is not well formed (lowercase words joined by -, with / for a namespace, like plan or ticket/abc-12).\n\nPrints the new id and file. JSON: {id, path, board, section, stack, order, tags}",
        after_long_help = "Examples:\n  wb --workspace boards --board garden note add --title \"Plant list\" --body \"Beans and peas.\"\n  wb --workspace boards --board garden note add --title \"Seed order\" --section Beds --tag plan\n  cat plan.md | wb --workspace boards --board garden note add --source \"garden diary\""
    )]
    Add {
        #[arg(
            long,
            help = "The note's title [default: a `# Heading` that starts the body]"
        )]
        title: Option<String>,
        #[arg(
            long,
            value_name = "SECTION",
            help = "The section: a path like `Beds / Spring`, an id, or `none` [default: the board's default section]"
        )]
        section: Option<String>,
        #[arg(long = "tag", value_name = "TAG", help = "A tag. Repeat for more")]
        tags: Vec<String>,
        #[arg(long, help = "Where the note came from, shown small on the card")]
        source: Option<String>,
        #[command(flatten)]
        body: BodyArgs,
    },
    #[command(
        about = "Replace a note's title or body, or add to its body",
        long_about = "Replace a note's title and/or body, or with --append add text to the end of its body as a new paragraph. The text comes from --body, --file (`-` for stdin) or stdin when it is not a terminal; empty stdin counts as no text.\n\nJSON: {id, path, title}",
        after_long_help = "Examples:\n  wb --workspace boards --board garden note edit n3 --title \"Plant list, spring\"\n  echo \"Add peas.\" | wb --workspace boards --board garden note edit n3 --append"
    )]
    Edit {
        #[arg(help = "The note: `id`, or `board#id`")]
        id: String,
        #[arg(long, help = "The new title")]
        title: Option<String>,
        #[command(flatten)]
        body: BodyArgs,
        #[arg(
            long,
            help = "Add the text to the end of the body instead of replacing it"
        )]
        append: bool,
    },
}

#[derive(Args)]
pub struct BodyArgs {
    #[arg(long, conflicts_with = "file", help = "The body text")]
    body: Option<String>,
    #[arg(
        long,
        value_name = "PATH",
        help = "Read the body from this file, `-` for stdin"
    )]
    file: Option<PathBuf>,
}

pub fn run(cx: &Context, command: NoteCommand) -> Result<()> {
    match command {
        NoteCommand::List { filter } => emit(cx.json, &list::list(cx, &filter)?[..], list::render),
        NoteCommand::Show { id } => emit(cx.json, &show::show(cx, &id)?, show::render),
        NoteCommand::Add {
            title,
            section,
            tags,
            source,
            body,
        } => {
            let request = add::NewNote {
                title,
                section,
                tags,
                source: source.unwrap_or_default(),
                body: read_body(body)?,
            };
            emit(cx.json, &add::add(cx, request)?, add::render)
        }
        NoteCommand::Edit {
            id,
            title,
            body,
            append,
        } => {
            let body = read_body(body)?;
            emit(
                cx.json,
                &edit::edit(cx, &id, title, body, append)?,
                edit::render,
            )
        }
    }
}

fn read_body(args: BodyArgs) -> Result<Option<String>> {
    if args.body.is_some() {
        return Ok(args.body);
    }
    match args.file {
        Some(path) if path.as_os_str() == STDIN_PATH => read_stdin().map(Some),
        Some(path) => fs::read_to_string(&path)
            .map(Some)
            .map_err(|error| Failure(format!("could not read {}: {error}", path.display()))),
        None if std::io::stdin().is_terminal() => Ok(None),
        None => Ok(Some(read_stdin()?).filter(|text| !text.trim().is_empty())),
    }
}

fn read_stdin() -> Result<String> {
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|error| Failure(format!("could not read stdin: {error}")))?;
    Ok(text)
}

fn note_ref(cx: &Context, text: &str) -> Result<(BoardRef, String)> {
    match text.trim().rsplit_once('#') {
        Some((board, id)) => Ok((cx.board_named(board)?, id.trim().to_owned())),
        None => Ok((cx.board()?, text.trim().to_owned())),
    }
}

fn not_found(board: &BoardRef, id: &str) -> Failure {
    Failure(format!(
        "there is no note {id} on board {}; `wb note list` shows the notes there are",
        board.id
    ))
}

fn live(board: &BoardRef, id: &str) -> Result<NoteFile> {
    let Ok(note_id) = NoteId::parse(id) else {
        return Err(not_found(board, id));
    };
    match board.board.note_file(&note_id) {
        Ok(file) => Ok(file),
        Err(Error::NoteNotFound { .. }) => Err(not_found(board, id)),
        Err(error) => Err(error.into()),
    }
}
