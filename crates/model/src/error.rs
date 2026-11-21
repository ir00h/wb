use alloc::string::String;

use serde::{Deserialize, Serialize};
use thiserror::Error;

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
