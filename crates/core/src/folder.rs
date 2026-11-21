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

impl NoteFile {
    fn is_named_for_its_id(&self) -> bool {
        is_named_for(&self.name, &self.note.id)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Scan {
    pub notes: Vec<NoteFile>,
    pub strays: Vec<String>,
}

enum Read {
    Note(Box<NoteFile>),
    Stray,
    Missing,
}

fn is_named_for(name: &str, id: &NoteId) -> bool {
    name.strip_suffix(NOTE_EXTENSION)
        .and_then(|stem| stem.strip_suffix(id.as_str()))
        .is_some_and(|rest| rest.ends_with('-'))
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

fn read(folder: &Path, name: String) -> Result<Read, Error> {
    let path = folder.join(&name);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Read::Missing),
        Err(error) if error.kind() == ErrorKind::InvalidData => return Ok(Read::Stray),
        Err(error) => return Err(io_failure(&path)(error)),
    };
    Ok(match parse(&text) {
        Parsed::Note(note) => Read::Note(Box::new(NoteFile {
            name,
            text,
            note: *note,
        })),
        Parsed::Stray { .. } => Read::Stray,
    })
}

pub fn scan(folder: &Path) -> Result<Scan, Error> {
    let mut scan = Scan::default();
    for name in note_file_names(folder)? {
        match read(folder, name.clone())? {
            Read::Note(file) => {
                let same_id = scan
                    .notes
                    .iter_mut()
                    .find(|other| other.note.id == file.note.id);
                match same_id {
                    None => scan.notes.push(*file),
                    Some(shown) if !shown.is_named_for_its_id() && file.is_named_for_its_id() => {
                        scan.strays.push(std::mem::replace(shown, *file).name);
                    }
                    Some(_) => scan.strays.push(name),
                }
            }
            Read::Stray => scan.strays.push(name),
            Read::Missing => {}
        }
    }
    scan.strays.sort();
    Ok(scan)
}

fn the_one_file(id: &NoteId, mut holding: Vec<NoteFile>) -> Result<Option<NoteFile>, Error> {
    if holding.len() > 1 {
        return Err(Error::DuplicateNote {
            note: id.clone(),
            files: holding.into_iter().map(|file| file.name).collect(),
        });
    }
    Ok(holding.pop())
}

fn holding(folder: &Path, id: &NoteId, names: Vec<String>) -> Result<Vec<NoteFile>, Error> {
    let mut holding = Vec::new();
    for name in names {
        if let Read::Note(file) = read(folder, name)?
            && &file.note.id == id
        {
            holding.push(*file);
        }
    }
    Ok(holding)
}

pub fn find(folder: &Path, id: &NoteId) -> Result<Option<NoteFile>, Error> {
    let everywhere = holding(folder, id, note_file_names(folder)?)?;
    let (named, other): (Vec<NoteFile>, Vec<NoteFile>) = everywhere
        .into_iter()
        .partition(NoteFile::is_named_for_its_id);
    the_one_file(id, if named.is_empty() { other } else { named })
}

pub fn trashed_ids(board: &Path) -> Result<Vec<NoteId>, Error> {
    let trashed = scan(&board.join(TRASH_FOLDER))?;
    Ok(trashed.notes.into_iter().map(|file| file.note.id).collect())
}
