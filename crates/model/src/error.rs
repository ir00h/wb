use alloc::string::String;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::{HUE_MAX, HUE_MIN};
use crate::id::{BoardId, NoteId};
use crate::sections::SectionRef;

#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Error {
    #[error("the workspace folder `{path}` does not exist")]
    WorkspaceNotFound { path: String },
    #[error("there is no board `{board}` in this workspace")]
    BoardNotFound { board: BoardId },
    #[error("there is no note `{note}` on this board")]
    NoteNotFound { note: NoteId },
    #[error(
        "the note `{note}` is in more than one file (`{}`); keep one and remove the others or give them another id",
        files.join("`, `")
    )]
    DuplicateNote { note: NoteId, files: Vec<String> },
    #[error("there is no section {section} on this board")]
    SectionNotFound { section: SectionRef },
    #[error("{problem}")]
    Invalid { problem: Invalid },
    #[error(
        "{what} is busy: another program has held it for {waited_ms} ms; nothing was changed, try again"
    )]
    Busy { what: String, waited_ms: u64 },
    #[error("the folder of `{path}` does not support file locks, so nothing was changed")]
    LocksUnsupported { path: String },
    #[error("`{path}` cannot be read as written: {message}")]
    Malformed { path: String, message: String },
    #[error("`{path}`: {message}")]
    Io { path: String, message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Invalid {
    #[error("a section needs a name")]
    EmptySectionName,
    #[error("a section called `{name}` already exists there; pick another name")]
    SectionNameTaken { name: String },
    #[error("a hue is a number from {HUE_MIN} to {HUE_MAX}")]
    HueOutOfRange,
    #[error("a board needs a name")]
    EmptyBoardName,
}
