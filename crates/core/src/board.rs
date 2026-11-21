use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use wb_model::{
    BOARD_FILE, BoardFile, BoardId, BoardInfo, BoardSnapshot, DraftSection, Error, Invalid, Note,
    NoteCard, NoteDraft, NoteEdit, NoteId, Op, Outcome, Parsed, Placement, SectionId, SectionNode,
    SectionRef, SectionTree, Tag, clean_body, clean_title, edit_note, file_name, parse, render,
    text,
};

use crate::failure::{io_failure, kept_changing, lock_failure, malformed};
use crate::folder::{self, NoteFile};
use crate::lock::{BoardLocks, Held, LockKey};
use crate::replace::{
    Created, Renamed, Replaced, Target, create_new, rename_note, replace_if_unchanged,
};
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
            Op::AddSection {
                name,
                parent,
                note,
                hue,
            } => self.add_section(&name, parent.as_ref(), &note, hue),
            Op::AddNote { draft } => self.add_note(&draft),
            Op::EditNote { note, edit } => self.edit_note(&note, &edit),
        }
    }

    fn sections(&self) -> Result<SectionTree, Error> {
        Ok(SectionTree::from_config(&config::read(&self.folder)?))
    }

    pub(crate) fn find(&self, id: &NoteId) -> Result<NoteFile, Error> {
        folder::find(&self.folder, id)?.ok_or_else(|| Error::NoteNotFound { note: id.clone() })
    }

    fn lock(&self, key: &LockKey, what: &str) -> Result<Held, Error> {
        self.locks
            .acquire(std::slice::from_ref(key), self.lock_deadline)
            .map_err(lock_failure(&self.folder, what))
    }

    fn add_section(
        &self,
        name: &str,
        parent: Option<&SectionRef>,
        note: &str,
        hue: Option<f64>,
    ) -> Result<Outcome, Error> {
        let held = self.lock(&LockKey::Config, "the board's sections")?;
        let listed_in_the_board_file: BTreeSet<String> = read_board_file(&self.folder)
            .map(|file| file.section_ids().map(str::to_owned).collect())
            .unwrap_or_default();
        let section = config::change(&held, |config| {
            let parent = match parent {
                Some(reference) => {
                    Some(section_named(&SectionTree::from_config(config), reference)?)
                }
                None => None,
            };
            let id = ids::new_section_id(|id| {
                listed_in_the_board_file.contains(id.as_str())
                    || config
                        .section_entries()
                        .iter()
                        .any(|entry| entry.id == id.as_str())
            });
            config
                .add_section(&id, name, parent.as_ref(), note, hue)
                .map_err(|problem| Error::Invalid { problem })?;
            let name = config
                .section_entries()
                .last()
                .map(|entry| entry.name.clone())
                .unwrap_or_default();
            Ok(SectionNode {
                id,
                name,
                hue,
                children: Vec::new(),
            })
        })?;
        Ok(Outcome::SectionAdded { section })
    }

    fn add_note(&self, draft: &NoteDraft) -> Result<Outcome, Error> {
        let config = config::read_for_a_write(&self.folder)?;
        let sections = SectionTree::from_config(&config);
        let section = match &draft.section {
            DraftSection::Named { section } => Some(section_named(&sections, section)?),
            DraftSection::NoSection => None,
            DraftSection::Default => config
                .default_section
                .as_deref()
                .and_then(|id| SectionId::parse(id).ok())
                .filter(|id| sections.contains(id)),
        };
        let tags = Tag::list(&draft.tags);
        if let Some(tag) = tags.iter().find(|tag| !tag.is_well_formed()) {
            return Err(Error::Invalid {
                problem: Invalid::MalformedTag {
                    tag: tag.to_string(),
                },
            });
        }
        let taken = self.taken_note_ids()?;
        loop {
            let id = ids::new_note_id(|id| taken.contains(id.as_str()));
            let held = self.lock(&LockKey::Note(id.clone()), "the new note")?;
            match folder::find(&self.folder, &id) {
                Ok(None) => {}
                Ok(Some(_)) | Err(Error::DuplicateNote { .. }) => continue,
                Err(error) => return Err(error),
            }
            let note = Note {
                placement: Placement {
                    section: section.as_ref().and_then(|id| sections.path_of(id)),
                    ..Placement::default()
                },
                tags: tags.clone(),
                source: text::trim(&draft.source).to_owned(),
                title: clean_title(&draft.title),
                body: clean_body(&draft.body),
                ..Note::new(id)
            };
            let (name, text) = (file_name(&note), render(&note));
            let target = Target::Note {
                id: &note.id,
                name: &name,
            };
            let path = self.folder.join(&name);
            match create_new(&held, target, text.as_bytes()).map_err(io_failure(&path))? {
                Created::Done => {
                    return Ok(Outcome::Applied {
                        note: NoteCard::of(&as_written(&text, note), &sections),
                    });
                }
                Created::Exists => {}
            }
        }
    }

    fn edit_note(&self, id: &NoteId, edit: &NoteEdit) -> Result<Outcome, Error> {
        let held = self.lock(&LockKey::Note(id.clone()), &format!("the note `{id}`"))?;
        let sections = self.sections()?;
        for _ in 0..config::ATTEMPTS {
            let found = self.find(id)?;
            let Ok(edited) = edit_note(&found.note, edit) else {
                return Ok(Outcome::Conflict {
                    current: NoteCard::of(&found.note, &sections),
                });
            };
            if edited == found.note {
                return Ok(Outcome::Applied {
                    note: NoteCard::of(&found.note, &sections),
                });
            }
            let text = render(&edited);
            match self.rewrite(&held, &found, &edited, &text)? {
                Replaced::Done => {
                    return Ok(Outcome::Applied {
                        note: NoteCard::of(&as_written(&text, edited), &sections),
                    });
                }
                Replaced::Changed { .. } => {}
                Replaced::Gone => return Err(Error::NoteNotFound { note: id.clone() }),
            }
        }
        Err(kept_changing(&self.folder))
    }

    fn rewrite(
        &self,
        held: &Held,
        found: &NoteFile,
        note: &Note,
        text: &str,
    ) -> Result<Replaced, Error> {
        let path = self.folder.join(&found.name);
        if found.text != text {
            let target = Target::Note {
                id: &note.id,
                name: &found.name,
            };
            let replaced =
                replace_if_unchanged(held, target, found.text.as_bytes(), text.as_bytes())
                    .map_err(io_failure(&path))?;
            if replaced != Replaced::Done {
                return Ok(replaced);
            }
        }
        let name = file_name(note);
        if name == found.name {
            return Ok(Replaced::Done);
        }
        match rename_note(held, &note.id, &found.name, &name).map_err(io_failure(&path))? {
            Renamed::Done | Renamed::Taken => Ok(Replaced::Done),
            Renamed::Gone => Ok(Replaced::Gone),
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

fn as_written(text: &str, note: Note) -> Note {
    match parse(text) {
        Parsed::Note(written) => *written,
        Parsed::Stray { .. } => note,
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
