use std::fs::{self, File, OpenOptions, Permissions};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};

use wb_model::{BOARD_FILE, CONFIG_FILE, NoteId};

use crate::lock::{Held, LockKey};
use crate::volume::{Creation, Replacement, Volume, platform};

const EXCHANGES: usize = 8;

#[derive(Debug, Clone, Copy)]
pub enum Target<'a> {
    BoardFile,
    Config,
    Note { id: &'a NoteId, name: &'a str },
}

impl Target<'_> {
    fn key(&self) -> LockKey {
        match self {
            Target::BoardFile | Target::Config => LockKey::Config,
            Target::Note { id, .. } => LockKey::Note((*id).clone()),
        }
    }

    fn path(&self, board: &Path) -> PathBuf {
        match self {
            Target::BoardFile => board.join(BOARD_FILE),
            Target::Config => board.join(CONFIG_FILE),
            Target::Note { name, .. } => board.join(name),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Replaced {
    Done,
    Changed { now: Vec<u8> },
    Gone,
    NotAPlainFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Created {
    Done,
    Exists,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Renamed {
    Done,
    Taken,
    Gone,
}

pub fn replace_if_unchanged(
    held: &Held,
    target: Target,
    read: &[u8],
    new: &[u8],
) -> io::Result<Replaced> {
    let temporary = held.temporary_file(&target.key())?;
    let path = target.path(held.board());
    let replaced = replace_unless_changed(held.volume(), &temporary, &path, read, new);
    let _ = fs::remove_file(&temporary);
    if matches!(replaced, Ok(Replaced::Done)) {
        sync_folder_of(&path);
    }
    replaced
}

pub fn create_new(held: &Held, target: Target, new: &[u8]) -> io::Result<Created> {
    let temporary = held.temporary_file(&target.key())?;
    let path = target.path(held.board());
    #[cfg(test)]
    hook::run(hook::Moment::BeforeTheCreation, &path);
    let created = match held.volume().creation {
        Creation::RenameWithoutReplacing => write_temporary(&temporary, new, None)
            .and_then(|()| platform::rename_without_replacing(&temporary, &path)),
        Creation::HardLink => {
            write_temporary(&temporary, new, None).and_then(|()| fs::hard_link(&temporary, &path))
        }
        Creation::CreateNew => write_in_place(&path, new),
    };
    let _ = fs::remove_file(&temporary);
    match created {
        Ok(()) => {
            sync_folder_of(&path);
            Ok(Created::Done)
        }
        Err(error) if error.kind() == ErrorKind::AlreadyExists => Ok(Created::Exists),
        Err(error) => Err(error),
    }
}

pub fn rename_note(held: &Held, id: &NoteId, from: &str, to: &str) -> io::Result<Renamed> {
    held.temporary_file(&LockKey::Note(id.clone()))?;
    let (from, to) = (held.board().join(from), held.board().join(to));
    #[cfg(test)]
    hook::run(hook::Moment::BeforeTheRename, &from);
    let renamed = match held.volume().creation {
        Creation::RenameWithoutReplacing => platform::rename_without_replacing(&from, &to),
        Creation::HardLink => fs::hard_link(&from, &to).and_then(|()| fs::remove_file(&from)),
        Creation::CreateNew => rename_after_a_look(&from, &to),
    };
    match renamed {
        Ok(()) => {
            sync_folder_of(&to);
            Ok(Renamed::Done)
        }
        Err(error) if error.kind() == ErrorKind::AlreadyExists => Ok(Renamed::Taken),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Renamed::Gone),
        Err(error) => Err(error),
    }
}

fn rename_after_a_look(from: &Path, to: &Path) -> io::Result<()> {
    if to.try_exists()? {
        return Err(ErrorKind::AlreadyExists.into());
    }
    fs::rename(from, to)
}

fn write_in_place(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    if written.is_err() {
        let _ = fs::remove_file(path);
    }
    written
}

fn look(path: &Path, read: &[u8]) -> io::Result<Option<Replaced>> {
    match fs::read(path) {
        Ok(on_disk) if on_disk == read => Ok(None),
        Ok(on_disk) => Ok(Some(Replaced::Changed { now: on_disk })),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Some(Replaced::Gone)),
        Err(error) if is_a_folder(&error) => Ok(Some(Replaced::NotAPlainFile)),
        Err(error) => Err(error),
    }
}

fn is_a_folder(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::IsADirectory | ErrorKind::DirectoryNotEmpty
    )
}

fn is_a_plain_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|entry| entry.file_type().is_file())
}

fn replace_unless_changed(
    volume: Volume,
    temporary: &Path,
    path: &Path,
    read: &[u8],
    new: &[u8],
) -> io::Result<Replaced> {
    let permissions = match fs::symlink_metadata(path) {
        Ok(entry) if !entry.file_type().is_file() => return Ok(Replaced::NotAPlainFile),
        Ok(entry) => Some(entry.permissions()),
        Err(_) => None,
    };
    write_temporary(temporary, new, permissions)?;
    #[cfg(test)]
    hook::run(hook::Moment::BeforeTheLastLook, path);
    if let Some(stop) = look(path, read)? {
        return Ok(stop);
    }
    #[cfg(test)]
    hook::run(hook::Moment::AfterTheLastLook, path);
    match volume.replacement {
        Replacement::Exchange => exchange(temporary, path, read, new),
        Replacement::RenameAfterALastLook => match fs::rename(temporary, path) {
            Ok(()) => Ok(Replaced::Done),
            Err(error) if is_a_folder(&error) => Ok(Replaced::NotAPlainFile),
            Err(error) => Err(error),
        },
    }
}

fn exchange(temporary: &Path, path: &Path, read: &[u8], new: &[u8]) -> io::Result<Replaced> {
    match platform::exchange(temporary, path) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Replaced::Gone),
        Err(error) => return Err(error),
    }
    if !is_a_plain_file(temporary) {
        platform::exchange(temporary, path)?;
        return Ok(Replaced::NotAPlainFile);
    }
    let mut put = new.to_vec();
    let mut taken_out = fs::read(temporary)?;
    if taken_out == read {
        return Ok(Replaced::Done);
    }
    for _ in 1..EXCHANGES {
        match platform::exchange(temporary, path) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Replaced::Gone),
            Err(error) => return Err(error),
        }
        let came_back = fs::read(temporary)?;
        if came_back == put {
            return Ok(Replaced::Changed { now: taken_out });
        }
        put = std::mem::replace(&mut taken_out, came_back);
    }
    Err(io::Error::other(
        "another program kept writing the file while it was being replaced",
    ))
}

fn write_temporary(
    temporary: &Path,
    bytes: &[u8],
    permissions: Option<Permissions>,
) -> io::Result<()> {
    let _ = fs::remove_file(temporary);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary)?;
    file.write_all(bytes)?;
    if let Some(permissions) = permissions {
        file.set_permissions(permissions)?;
    }
    file.sync_all()
}

fn sync_folder_of(path: &Path) {
    if let Some(folder) = path.parent()
        && let Ok(folder) = File::open(folder)
    {
        let _ = folder.sync_all();
    }
}

#[cfg(test)]
pub mod hook {
    use std::cell::RefCell;
    use std::path::Path;

    type Action = Box<dyn FnOnce(&Path)>;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Moment {
        BeforeTheLastLook,
        AfterTheLastLook,
        BeforeTheCreation,
        BeforeTheRename,
        AfterTheListing,
    }

    thread_local! {
        static NEXT: RefCell<Option<(Moment, Action)>> = const { RefCell::new(None) };
    }

    pub fn at_the_next(moment: Moment, action: impl FnOnce(&Path) + 'static) {
        NEXT.with(|hook| *hook.borrow_mut() = Some((moment, Box::new(action))));
    }

    pub fn before_the_next_exchange(action: impl FnOnce(&Path) + 'static) {
        at_the_next(Moment::AfterTheLastLook, action);
    }

    pub(crate) fn run(now: Moment, path: &Path) {
        let due = NEXT.with(|hook| {
            let mut hook = hook.borrow_mut();
            match hook.take() {
                Some((moment, action)) if moment == now => Some(action),
                other => {
                    *hook = other;
                    None
                }
            }
        });
        if let Some(action) = due {
            action(path);
        }
    }
}
