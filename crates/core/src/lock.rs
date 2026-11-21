use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use thiserror::Error;
use wb_model::NoteId;

pub const LOCKS_FOLDER: &str = ".locks";
const CONFIG_SIDECAR: &str = "board.config.lock";
const POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum LockKey {
    Config,
    Note(NoteId),
}

impl LockKey {
    fn sidecar(&self) -> String {
        match self {
            LockKey::Config => CONFIG_SIDECAR.to_owned(),
            LockKey::Note(id) => format!("{id}.lock"),
        }
    }
}

#[derive(Debug, Error)]
pub enum LockError {
    #[error("{key:?} is held by another program, waited {waited:?}")]
    Busy { key: LockKey, waited: Duration },
    #[error("`{}` is on a file system without file locks", path.display())]
    Unsupported { path: PathBuf },
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Debug)]
pub struct Held {
    keys: Vec<LockKey>,
    files: Vec<File>,
}

impl Held {
    #[must_use]
    pub fn covers(&self, key: &LockKey) -> bool {
        self.keys.contains(key)
    }
}

#[derive(Debug, Clone)]
pub struct BoardLocks {
    folder: PathBuf,
}

impl BoardLocks {
    #[must_use]
    pub fn of_board(board: &Path) -> BoardLocks {
        BoardLocks {
            folder: board.join(LOCKS_FOLDER),
        }
    }

    pub fn acquire(&self, keys: &[LockKey], deadline: Duration) -> Result<Held, LockError> {
        let mut sorted = keys.to_vec();
        sorted.sort();
        sorted.dedup();
        fs::create_dir_all(&self.folder)?;
        let started = Instant::now();
        let mut held = Held {
            keys: Vec::new(),
            files: Vec::new(),
        };
        for key in sorted {
            let path = self.folder.join(key.sidecar());
            let sidecar = OpenOptions::new().create(true).append(true).open(&path)?;
            loop {
                match sidecar.try_lock() {
                    Ok(()) => break,
                    Err(TryLockError::WouldBlock) => {
                        let waited = started.elapsed();
                        if waited >= deadline {
                            return Err(LockError::Busy { key, waited });
                        }
                        thread::sleep(POLL_INTERVAL);
                    }
                    Err(TryLockError::Error(error))
                        if error.kind() == io::ErrorKind::Unsupported =>
                    {
                        return Err(LockError::Unsupported { path });
                    }
                    Err(TryLockError::Error(error)) => return Err(LockError::Io(error)),
                }
            }
            held.keys.push(key);
            held.files.push(sidecar);
        }
        Ok(held)
    }
}

static TEMPORARY_FILES: AtomicU64 = AtomicU64::new(0);

fn temporary_path(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.subsec_nanos());
    let count = TEMPORARY_FILES.fetch_add(1, Ordering::Relaxed);
    target.with_file_name(format!(
        ".{name}.{}-{count}-{nanos}.tmp",
        std::process::id()
    ))
}

pub fn replace_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = temporary_path(path);
    let written = write_new(&temporary, bytes).and_then(|()| fs::rename(&temporary, path));
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    written?;
    if let Some(folder) = path.parent()
        && let Ok(folder) = File::open(folder)
    {
        let _ = folder.sync_all();
    }
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}
