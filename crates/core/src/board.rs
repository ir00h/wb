use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use wb_model::{
    BOARD_FILE, BoardFile, BoardId, BoardInfo, BoardSnapshot, Error, Note, NoteCard, NoteDraft,
    NoteEdit, NoteId, Op, Outcome, Parsed, Placement, SectionId, SectionNode, SectionRef,
    SectionTree, Tag, clean_body, clean_title, edit_note, file_name, parse, render,
};

use crate::failure::{io_failure, lock_failure, malformed};
use crate::folder::{self, NoteFile};
use crate::lock::{BoardLocks, Held, LockKey, replace_file};
use crate::{config, ids};

pub(crate) fn read_board_file(folder: &Path) -> Result<BoardFile, Error> {
    let path = folder.join(BOARD_FILE);
    let text = fs::read_to_string(&path).map_err(io_failure(&path))?;
    BoardFile::parse(&text).map_err(malformed(&path))
}

#[derive(Debug, Clone)]
pub struct Board {
    id: BoardId,
    folder: PathBuf,
    locks: BoardLocks,
    lock_deadline: Duration,
}

impl Board {
    pub(crate) fn at(id: BoardId, folder: PathBuf, lock_deadline: Duration) -> Board {
        Board {
            id,
            locks: BoardLocks::of_board(&folder),
            folder,
            lock_deadline,
        }
    }

    #[must_use]
    pub fn id(&self) -> &BoardId {
        &self.id
    }

    pub fn snapshot(&self) -> Result<BoardSnapshot, Error> {
        let name = read_board_file(&self.folder)?.name;
        let sections = self.sections()?;
        let scan = folder::scan(&self.folder)?;
        Ok(BoardSnapshot {
            board: BoardInfo {
                id: self.id.clone(),
                name,
            },
            sections: sections.nodes(),
            notes: scan
                .notes
                .iter()
                .map(|file| NoteCard::of(&file.note, &sections))
                .collect(),
            strays: scan.strays,
        })
    }

    pub fn note(&self, id: &NoteId) -> Result<Note, Error> {
        Ok(self.find(id)?.note)
    }

    pub fn apply(&self, op: Op) -> Result<Outcome, Error> {
        match op {
            Op::AddSection { name, parent } => self.add_section(&name, parent.as_ref()),
            Op::AddNote { draft } => self.add_note(&draft),
            Op::EditNote { note, edit } => self.edit_note(&note, &edit),
        }
    }

    fn sections(&self) -> Result<SectionTree, Error> {
        Ok(SectionTree::from_config(&config::read(&self.folder)?))
    }

    fn find(&self, id: &NoteId) -> Result<NoteFile, Error> {
        folder::find(&self.folder, id)?.ok_or_else(|| Error::NoteNotFound { note: id.clone() })
    }

    fn lock(&self, key: &LockKey, what: &str) -> Result<Held, Error> {
        self.locks
            .acquire(std::slice::from_ref(key), self.lock_deadline)
            .map_err(lock_failure(&self.folder, what))
    }

    fn add_section(&self, name: &str, parent: Option<&SectionRef>) -> Result<Outcome, Error> {
        let held = self.lock(&LockKey::Config, "the board's sections")?;
        let mut config = config::read(&self.folder)?;
        let parent = match parent {
            Some(reference) => Some(section_named(
                &SectionTree::from_config(&config),
                reference,
            )?),
            None => None,
        };
        let mut taken: BTreeSet<String> = config
            .section_entries()
            .iter()
            .map(|entry| entry.id.clone())
            .collect();
        if let Ok(file) = read_board_file(&self.folder) {
            taken.extend(file.section_ids().map(str::to_owned));
        }
        let id = ids::new_section_id(|id| taken.contains(id.as_str()));
        config
            .add_section(&id, name, parent.as_ref())
            .map_err(|problem| Error::Invalid { problem })?;
        config::write(&self.folder, &config, &held)?;
        let name = config
            .section_entries()
            .last()
            .map(|entry| entry.name.clone())
            .unwrap_or_default();
        Ok(Outcome::SectionAdded {
            section: SectionNode {
                id,
                name,
                hue: None,
                children: Vec::new(),
            },
        })
    }

    fn add_note(&self, draft: &NoteDraft) -> Result<Outcome, Error> {
        let config = config::read(&self.folder)?;
        let sections = SectionTree::from_config(&config);
        let section = match &draft.section {
            Some(reference) => Some(section_named(&sections, reference)?),
            None => config
                .default_section
                .as_deref()
                .and_then(|id| SectionId::parse(id).ok())
                .filter(|id| sections.contains(id)),
        };
        let taken = self.taken_note_ids()?;
        loop {
            let id = ids::new_note_id(|id| taken.contains(id.as_str()));
            let held = self.lock(&LockKey::Note(id.clone()), "the new note")?;
            if folder::find(&self.folder, &id)?.is_some() {
                continue;
            }
            let note = Note {
                placement: Placement {
                    section: section.as_ref().and_then(|id| sections.path_of(id)),
                    ..Placement::default()
                },
                tags: Tag::list(&draft.tags),
                title: clean_title(&draft.title),
                body: clean_body(&draft.body),
                ..Note::new(id)
            };
            let written = self.write_note(&held, None, &note)?;
            return Ok(Outcome::Applied {
                note: NoteCard::of(&written, &sections),
            });
        }
    }

    fn edit_note(&self, id: &NoteId, edit: &NoteEdit) -> Result<Outcome, Error> {
        let held = self.lock(&LockKey::Note(id.clone()), &format!("the note `{id}`"))?;
        let found = self.find(id)?;
        let sections = self.sections()?;
        let Ok(edited) = edit_note(&found.note, edit) else {
            return Ok(Outcome::Conflict {
                current: NoteCard::of(&found.note, &sections),
            });
        };
        let now = if edited == found.note {
            found.note
        } else {
            self.write_note(&held, Some(&found), &edited)?
        };
        Ok(Outcome::Applied {
            note: NoteCard::of(&now, &sections),
        })
    }

    fn write_note(
        &self,
        held: &Held,
        current: Option<&NoteFile>,
        note: &Note,
    ) -> Result<Note, Error> {
        debug_assert!(held.covers(&LockKey::Note(note.id.clone())));
        let text = render(note);
        let mut path = self.folder.join(file_name(note));
        if let Some(current) = current {
            let current_path = self.folder.join(&current.name);
            if current_path != path && !path.exists() {
                fs::rename(&current_path, &path).map_err(io_failure(&current_path))?;
            } else {
                path = current_path;
            }
        }
        if current.is_none_or(|current| current.text != text) {
            replace_file(&path, text.as_bytes()).map_err(io_failure(&path))?;
        }
        match parse(&text) {
            Parsed::Note(written) => Ok(*written),
            Parsed::Stray { .. } => Ok(note.clone()),
        }
    }

    fn taken_note_ids(&self) -> Result<BTreeSet<String>, Error> {
        let mut taken: BTreeSet<String> = folder::scan(&self.folder)?
            .notes
            .into_iter()
            .map(|file| file.note.id.into())
            .collect();
        taken.extend(
            folder::trashed_ids(&self.folder)?
                .into_iter()
                .map(String::from),
        );
        if let Ok(file) = read_board_file(&self.folder) {
            taken.extend(file.card_ids().map(str::to_owned));
        }
        Ok(taken)
    }
}

fn section_named(sections: &SectionTree, reference: &SectionRef) -> Result<SectionId, Error> {
    sections
        .find(reference)
        .cloned()
        .ok_or_else(|| Error::SectionNotFound {
            section: reference.clone(),
        })
}
