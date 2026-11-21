use alloc::string::String;

use serde::{Deserialize, Serialize};
use thiserror::Error;

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
    #[error("a board needs a name")]
    EmptyBoardName,
    #[error("a note needs a title or a body")]
    EmptyNote,
}
