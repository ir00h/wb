mod board;
mod context;
mod error;
mod note;
mod notes;
mod output;
mod section;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

use board::BoardCommand;
use context::Context;
use note::NoteCommand;
use section::SectionCommand;

const ABOUT: &str = "The whiteboard from the command line: boards, sections and notes, written as the files the Whiteboard app reads.";

const LONG_ABOUT: &str = "\
The whiteboard from the command line: boards, sections and notes, written as the files the
Whiteboard app reads. The app may be running on the same board at the same time.

wb owns meaning, the app owns geometry: wb writes note files and board.config.json, never a
position. A note that names a section is moved into it by the app; a section added here is drawn
by the app.

Which workspace: --workspace, else $WHITEBOARD_WORKSPACE. One of them must be given.

Which board: --board <id> (the board's folder path in the workspace, such as garden or
kitchen/recipes), else $WB_BOARD.

Output is text for people; --json prints JSON for programs. Errors are one sentence on stderr
with exit code 1 (2 for wrong arguments).";

const EXAMPLES: &str = "\
Examples:
  wb --workspace boards board list
  wb --workspace boards board add Garden
  wb --workspace boards --board garden section add Beds
  wb --workspace boards --board garden section add Spring --parent Beds
  echo \"Tomatoes, basil and beans.\" | wb --workspace boards --board garden note add --title \"Plant list\" --section Beds --tag plan
  wb --workspace boards --board garden note list --tag plan
  wb --workspace boards --board garden note edit n3 --append --body \"Add peas.\"";

#[derive(Parser)]
#[command(
    name = "wb",
    version,
    about = ABOUT,
    long_about = LONG_ABOUT,
    after_long_help = EXAMPLES,
    propagate_version = true
)]
struct Cli {
    #[command(flatten)]
    global: Global,
    #[command(subcommand)]
    command: Command,
}

#[derive(Args)]
struct Global {
    #[arg(
        long,
        global = true,
        value_name = "PATH",
        help = "The workspace folder [default: $WHITEBOARD_WORKSPACE]"
    )]
    workspace: Option<PathBuf>,
    #[arg(
        long,
        global = true,
        value_name = "ID",
        help = "The board: its folder path in the workspace, like `kitchen/recipes` [default: $WB_BOARD]"
    )]
    board: Option<String>,
    #[arg(
        long,
        global = true,
        help = "Print JSON for programs instead of text for people"
    )]
    json: bool,
}

#[derive(Subcommand)]
enum Command {
    #[command(subcommand, about = "List, show and create boards")]
    Board(BoardCommand),
    #[command(subcommand, about = "List and add the sections of a board")]
    Section(SectionCommand),
    #[command(subcommand, about = "List, show, add and edit notes")]
    Note(NoteCommand),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let outcome =
        Context::new(cli.global.workspace, cli.global.board, cli.global.json).and_then(|cx| {
            match cli.command {
                Command::Board(command) => board::run(&cx, command),
                Command::Section(command) => section::run(&cx, command),
                Command::Note(command) => note::run(&cx, command),
            }
        });
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => failure.report(),
    }
}
