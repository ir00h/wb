#![cfg_attr(not(any(test, feature = "ts")), no_std)]

extern crate alloc;

mod board_file;
mod config;
mod error;
mod id;
mod note;
mod section_path;
mod sections;
mod tag;
pub mod text;

pub use board_file::{BOARD_FILE, BOARD_FORMAT_VERSION, BoardFile};
pub use config::{BoardConfig, CONFIG_FILE, CONFIG_VERSION, SectionEntry, TagColour};
pub use error::Invalid;
pub use id::{BOARD_FOLDER_MAX_BYTES, BoardId, ITEM_ID_MAX_CHARS, IdError, NoteId, SectionId};
pub use note::{
    KNOWN_KEYS, Note, NoteKind, Parsed, Placement, clean_body, clean_title, file_name, parse,
    render, slug,
};
pub use section_path::{
    EmptySectionPath, SECTION_SEPARATOR, SectionPath, same_section_name, section_name,
};
pub use sections::{SectionNode, SectionRef, SectionTree};
pub use tag::{EmptyTag, Tag};
