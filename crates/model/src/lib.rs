#![cfg_attr(not(any(test, feature = "ts")), no_std)]

extern crate alloc;

mod id;

pub use id::{BOARD_FOLDER_MAX_BYTES, BoardId, ITEM_ID_MAX_CHARS, IdError, NoteId, SectionId};
