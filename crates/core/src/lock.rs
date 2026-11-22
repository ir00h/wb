use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use thiserror::Error;
use wb_model::NoteId;

use crate::volume::Volume;

pub const LOCKS_FOLDER: &str = ".locks";
const CONFIG_STEM: &str = "board.config";
const SIDECAR_EXTENSION: &str = "lock";
const TEMPORARY_EXTENSION: &str = "tmp";
const PROBE_EXTENSION: &str = "probe";
const POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum LockKey {
    Config,
    Note(NoteId),
}

impl LockKey {
    fn file_name(&self, extension: &str) -> String {
        match self {
            LockKey::Config => format!("{CONFIG_STEM}.{extension}"),
            LockKey::Note(id) => format!("{id}.{extension}"),
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
    board: PathBuf,
    keys: Vec<LockKey>,
    files: Vec<File>,
    volume: Volume,
}

impl Held {
    #[must_use]
    pub fn covers(&self, key: &LockKey) -> bool {
        self.keys.contains(key)
    }

    pub(crate) fn board(&self) -> &Path {
        &self.board
    }

    pub(crate) fn volume(&self) -> Volume {
        self.volume
    }

    pub(crate) fn temporary_file(&self, key: &LockKey) -> io::Result<PathBuf> {
        if !self.covers(key) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("this writer does not hold the lock {key:?}"),
            ));
        }
        Ok(self
            .board
            .join(LOCKS_FOLDER)
            .join(key.file_name(TEMPORARY_EXTENSION)))
    }
}

#[derive(Debug, Clone)]
pub struct BoardLocks {
    board: PathBuf,
    volume: Arc<OnceLock<Volume>>,
}

impl BoardLocks {
    #[must_use]
    pub fn of_board(board: &Path) -> BoardLocks {
        BoardLocks::on_volume(board, None)
    }

    pub(crate) fn on_volume(board: &Path, volume: Option<Volume>) -> BoardLocks {
        BoardLocks {
            board: board.to_path_buf(),
            volume: Arc::new(volume.map(OnceLock::from).unwrap_or_default()),
        }
    }

    pub(crate) fn board(&self) -> &Path {
        &self.board
    }

    pub(crate) fn was_ever_taken(&self, key: &LockKey) -> bool {
        self.board
            .join(LOCKS_FOLDER)
            .join(key.file_name(SIDECAR_EXTENSION))
            .exists()
    }

    fn volume(&self, folder: &Path, key: &LockKey) -> io::Result<Volume> {
        if let Some(volume) = self.volume.get() {
            return Ok(*volume);
        }
        let probed = Volume::probe(
            &folder.join(key.file_name(TEMPORARY_EXTENSION)),
            &folder.join(key.file_name(PROBE_EXTENSION)),
        )?;
        Ok(*self.volume.get_or_init(|| probed))
    }

    pub fn acquire(&self, keys: &[LockKey], deadline: Duration) -> Result<Held, LockError> {
        let mut sorted = keys.to_vec();
        sorted.sort();
        sorted.dedup();
        let folder = self.board.join(LOCKS_FOLDER);
        fs::create_dir_all(&folder)?;
        let started = Instant::now();
        let mut held = Held {
            board: self.board.clone(),
            keys: Vec::new(),
            files: Vec::new(),
            volume: Volume::WITHOUT_EXCHANGE,
        };
        for key in sorted {
            let path = folder.join(key.file_name(SIDECAR_EXTENSION));
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
            let _ = fs::remove_file(folder.join(key.file_name(TEMPORARY_EXTENSION)));
            let _ = fs::remove_file(folder.join(key.file_name(PROBE_EXTENSION)));
            held.keys.push(key);
            held.files.push(sidecar);
        }
        if let Some(key) = held.keys.first() {
            held.volume = self.volume(&folder, key)?;
        }
        Ok(held)
    }
}
