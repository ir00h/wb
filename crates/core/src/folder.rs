use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use wb_model::{Error, Note, NoteId, Parsed, parse};

use crate::failure::io_failure;

pub const TRASH_FOLDER: &str = ".trash";
const NOTE_EXTENSION: &str = ".md";

#[derive(Debug, Clone, PartialEq)]
pub struct NoteFile {
    pub name: String,
    pub text: String,
    pub note: Note,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Scan {
    pub notes: Vec<NoteFile>,
    pub strays: Vec<String>,
}

fn note_file_names(folder: &Path) -> Result<Vec<String>, Error> {
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io_failure(folder)(error)),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(io_failure(folder))?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name.ends_with(NOTE_EXTENSION) && !name.starts_with('.') && entry.path().is_file() {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

pub fn scan(folder: &Path) -> Result<Scan, Error> {
    let mut scan = Scan::default();
    for name in note_file_names(folder)? {
        let path = folder.join(&name);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) if error.kind() == ErrorKind::InvalidData => {
                scan.strays.push(name);
                continue;
            }
            Err(error) => return Err(io_failure(&path)(error)),
        };
        match parse(&text) {
            Parsed::Note(note) if scan.notes.iter().all(|other| other.note.id != note.id) => {
                scan.notes.push(NoteFile {
                    name,
                    text,
                    note: *note,
                });
            }
            Parsed::Note(_) | Parsed::Stray { .. } => scan.strays.push(name),
        }
    }
    Ok(scan)
}

pub fn find(folder: &Path, id: &NoteId) -> Result<Option<NoteFile>, Error> {
    Ok(scan(folder)?
        .notes
        .into_iter()
        .find(|file| &file.note.id == id))
}

pub fn trashed_ids(board: &Path) -> Result<Vec<NoteId>, Error> {
    let trashed = scan(&board.join(TRASH_FOLDER))?;
    Ok(trashed.notes.into_iter().map(|file| file.note.id).collect())
}
