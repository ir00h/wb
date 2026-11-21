use alloc::borrow::ToOwned;
use alloc::string::String;
use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const ITEM_ID_MAX_CHARS: usize = 40;
pub const BOARD_FOLDER_MAX_BYTES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum IdError {
    #[error("an id cannot be empty")]
    Empty,
    #[error("an id holds at most {max} characters, this one holds {actual}")]
    TooLong { max: usize, actual: usize },
    #[error("an id holds only letters, digits, `_` and `-`, not `{0}`")]
    InvalidCharacter(char),
    #[error("a board id cannot hold an empty folder name")]
    EmptyFolder,
    #[error("the folder name `{folder}` is longer than {max} bytes")]
    FolderTooLong { folder: String, max: usize },
    #[error(
        "the folder name `{folder}` must start with a lowercase letter or digit and hold only lowercase letters, digits and `-`"
    )]
    InvalidFolder { folder: String },
}

fn parse_item_id(text: &str) -> Result<(), IdError> {
    if text.is_empty() {
        return Err(IdError::Empty);
    }
    if let Some(invalid) = text
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || *c == '_' || *c == '-'))
    {
        return Err(IdError::InvalidCharacter(invalid));
    }
    if text.len() > ITEM_ID_MAX_CHARS {
        return Err(IdError::TooLong {
            max: ITEM_ID_MAX_CHARS,
            actual: text.len(),
        });
    }
    Ok(())
}

fn is_folder_name(folder: &str) -> bool {
    let mut chars = folder.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn parse_board_id(text: &str) -> Result<(), IdError> {
    if text.is_empty() {
        return Err(IdError::Empty);
    }
    for folder in text.split('/') {
        if folder.is_empty() {
            return Err(IdError::EmptyFolder);
        }
        if !is_folder_name(folder) {
            return Err(IdError::InvalidFolder {
                folder: folder.to_owned(),
            });
        }
        if folder.len() > BOARD_FOLDER_MAX_BYTES {
            return Err(IdError::FolderTooLong {
                folder: folder.to_owned(),
                max: BOARD_FOLDER_MAX_BYTES,
            });
        }
    }
    Ok(())
}

macro_rules! id_type {
    ($name:ident, $brand:literal, $rule:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        #[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(type = $brand))]
        pub struct $name(String);

        impl $name {
            pub fn parse(text: &str) -> Result<Self, IdError> {
                $rule(text)?;
                Ok(Self(text.to_owned()))
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl FromStr for $name {
            type Err = IdError;

            fn from_str(text: &str) -> Result<Self, IdError> {
                Self::parse(text)
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdError;

            fn try_from(text: String) -> Result<Self, IdError> {
                $rule(&text)?;
                Ok(Self(text))
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> String {
                id.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

id_type!(
    NoteId,
    "string & { readonly __brand: \"NoteId\" }",
    parse_item_id
);
id_type!(
    SectionId,
    "string & { readonly __brand: \"SectionId\" }",
    parse_item_id
);
id_type!(
    BoardId,
    "string & { readonly __brand: \"BoardId\" }",
    parse_board_id
);
