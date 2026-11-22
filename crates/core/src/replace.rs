use std::fs::{self, File, OpenOptions, Permissions};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};

use wb_model::{CONFIG_FILE, NoteId};

use crate::lock::{Held, LockKey};

const EXCHANGES: usize = 8;

#[derive(Debug, Clone, Copy)]
pub enum Target<'a> {
    Config,
    Note { id: &'a NoteId, name: &'a str },
}

impl Target<'_> {
    fn key(&self) -> LockKey {
        match self {
            Target::Config => LockKey::Config,
            Target::Note { id, .. } => LockKey::Note((*id).clone()),
        }
    }

    fn path(&self, board: &Path) -> PathBuf {
        match self {
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
    let replaced = exchange_unless_changed(&temporary, &path, read, new);
    let _ = fs::remove_file(&temporary);
    if matches!(replaced, Ok(Replaced::Done)) {
        sync_folder_of(&path);
    }
    replaced
}

pub fn create_new(held: &Held, target: Target, new: &[u8]) -> io::Result<Created> {
    let temporary = held.temporary_file(&target.key())?;
    create_through(&temporary, &target.path(held.board()), new)
}

pub fn create_in_a_folder_made_for_it(path: &Path, new: &[u8]) -> io::Result<Created> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    create_through(&path.with_file_name(format!(".{name}.tmp")), path, new)
}

pub fn rename_note(held: &Held, id: &NoteId, from: &str, to: &str) -> io::Result<Renamed> {
    held.temporary_file(&LockKey::Note(id.clone()))?;
    let (from, to) = (held.board().join(from), held.board().join(to));
    match platform::rename_unless_taken(&from, &to) {
        Ok(()) => {
            sync_folder_of(&to);
            Ok(Renamed::Done)
        }
        Err(error) if error.kind() == ErrorKind::AlreadyExists => Ok(Renamed::Taken),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Renamed::Gone),
        Err(error) => Err(error),
    }
}

fn create_through(temporary: &Path, path: &Path, new: &[u8]) -> io::Result<Created> {
    let created = write_temporary(temporary, new, None)
        .and_then(|()| platform::rename_unless_taken(temporary, path));
    match created {
        Ok(()) => {
            sync_folder_of(path);
            Ok(Created::Done)
        }
        Err(error) => {
            let _ = fs::remove_file(temporary);
            if error.kind() == ErrorKind::AlreadyExists {
                Ok(Created::Exists)
            } else {
                Err(error)
            }
        }
    }
}

fn exchange_unless_changed(
    temporary: &Path,
    path: &Path,
    read: &[u8],
    new: &[u8],
) -> io::Result<Replaced> {
    let permissions = match fs::metadata(path) {
        Ok(replaced) => replaced.permissions(),
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Replaced::Gone),
        Err(error) => return Err(error),
    };
    write_temporary(temporary, new, Some(permissions))?;
    match fs::read(path) {
        Ok(on_disk) if on_disk == read => {}
        Ok(on_disk) => return Ok(Replaced::Changed { now: on_disk }),
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Replaced::Gone),
        Err(error) => return Err(error),
    }
    #[cfg(test)]
    hook::run(path);
    match platform::exchange(temporary, path) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Replaced::Gone),
        Err(error) if platform::cannot_exchange(&error) => {
            return rename_after_a_last_look(temporary, path, read);
        }
        Err(error) => return Err(error),
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

fn rename_after_a_last_look(temporary: &Path, path: &Path, read: &[u8]) -> io::Result<Replaced> {
    match fs::read(path) {
        Ok(now) if now == read => fs::rename(temporary, path).map(|()| Replaced::Done),
        Ok(now) => Ok(Replaced::Changed { now }),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Replaced::Gone),
        Err(error) => Err(error),
    }
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

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod platform {
    use std::io::{self, ErrorKind};
    use std::path::Path;

    use rustix::fs::{CWD, RenameFlags, renameat_with};

    pub fn exchange(one: &Path, other: &Path) -> io::Result<()> {
        Ok(renameat_with(CWD, one, CWD, other, RenameFlags::EXCHANGE)?)
    }

    pub fn cannot_exchange(error: &io::Error) -> bool {
        matches!(
            error.kind(),
            ErrorKind::Unsupported | ErrorKind::InvalidInput
        )
    }

    pub fn rename_unless_taken(from: &Path, to: &Path) -> io::Result<()> {
        match renameat_with(CWD, from, CWD, to, RenameFlags::NOREPLACE) {
            Ok(()) => Ok(()),
            Err(error) if cannot_exchange(&error.into()) => super::rename_after_a_look(from, to),
            Err(error) => Err(error.into()),
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod platform {
    use std::io::{self, ErrorKind};
    use std::path::Path;

    pub fn exchange(_one: &Path, _other: &Path) -> io::Result<()> {
        Err(ErrorKind::Unsupported.into())
    }

    pub fn cannot_exchange(error: &io::Error) -> bool {
        error.kind() == ErrorKind::Unsupported
    }

    pub fn rename_unless_taken(from: &Path, to: &Path) -> io::Result<()> {
        super::rename_after_a_look(from, to)
    }
}

fn rename_after_a_look(from: &Path, to: &Path) -> io::Result<()> {
    if to.try_exists()? {
        return Err(ErrorKind::AlreadyExists.into());
    }
    fs::rename(from, to)
}

#[cfg(test)]
pub mod hook {
    use std::cell::RefCell;
    use std::path::Path;

    type Action = Box<dyn FnOnce(&Path)>;

    thread_local! {
        static BEFORE_THE_EXCHANGE: RefCell<Option<Action>> = const { RefCell::new(None) };
    }

    pub fn before_the_next_exchange(action: impl FnOnce(&Path) + 'static) {
        BEFORE_THE_EXCHANGE.with(|hook| *hook.borrow_mut() = Some(Box::new(action)));
    }

    pub(super) fn run(path: &Path) {
        let action = BEFORE_THE_EXCHANGE.with(|hook| hook.borrow_mut().take());
        if let Some(action) = action {
            action(path);
        }
    }
}
