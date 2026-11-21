use alloc::string::String;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::id::{BoardId, NoteId, SectionId};
use crate::note::Note;
use crate::sections::{SectionNode, SectionRef, SectionTree};
use crate::tag::Tag;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BoardInfo {
    pub id: BoardId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BoardSnapshot {
    pub board: BoardInfo,
    pub sections: Vec<SectionNode>,
    pub notes: Vec<NoteCard>,
    pub strays: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct NoteCard {
    pub id: NoteId,
    pub title: String,
    pub body: String,
    pub tags: Vec<Tag>,
    pub section: Option<SectionId>,
}

impl NoteCard {
    #[must_use]
    pub fn of(note: &Note, sections: &SectionTree) -> NoteCard {
        NoteCard {
            id: note.id.clone(),
            title: note.title.clone(),
            body: note.body.clone(),
            tags: note.tags.clone(),
            section: note
                .placement
                .section
                .as_ref()
                .and_then(|path| sections.resolve(path))
                .cloned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct NoteDraft {
    pub title: String,
    pub body: String,
    pub section: DraftSection,
    pub tags: Vec<String>,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum DraftSection {
    #[default]
    Default,
    NoSection,
    Named {
        section: SectionRef,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Op {
    AddSection {
        name: String,
        parent: Option<SectionRef>,
        note: String,
        hue: Option<f64>,
    },
    AddNote {
        draft: NoteDraft,
    },
    EditNote {
        note: NoteId,
        edit: NoteEdit,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct NoteEdit {
    pub title: Option<Change<String>>,
    pub body: Option<BodyChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Change<T> {
    pub base: Option<T>,
    pub to: T,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum BodyChange {
    Set(Change<String>),
    Append { text: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Outcome {
    Applied { note: NoteCard },
    Conflict { current: NoteCard },
    SectionAdded { section: SectionNode },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum BoardEvent {
    NoteChanged { note: NoteCard },
    NoteRemoved { note: NoteId },
    SectionsChanged { sections: Vec<SectionNode> },
}

#[must_use]
pub fn diff(before: &BoardSnapshot, after: &BoardSnapshot) -> Vec<BoardEvent> {
    let mut events = Vec::new();
    if before.sections != after.sections {
        events.push(BoardEvent::SectionsChanged {
            sections: after.sections.clone(),
        });
    }
    for note in &after.notes {
        if !before.notes.contains(note) {
            events.push(BoardEvent::NoteChanged { note: note.clone() });
        }
    }
    for note in &before.notes {
        if !after.notes.iter().any(|kept| kept.id == note.id) {
            events.push(BoardEvent::NoteRemoved {
                note: note.id.clone(),
            });
        }
    }
    events
}
