use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::Duration;

use wb_model::{BOARD_FILE, BoardFile, BoardId, BoardInfo, Error, Invalid, slug, text};

use crate::board::{Board, read_board_file};
use crate::failure::{io_failure, lock_failure, malformed};
use crate::lock::{BoardLocks, LockKey};
use crate::replace::{Target, create_new};
use crate::volume::Volume;

pub const DEFAULT_LOCK_DEADLINE: Duration = Duration::from_secs(10);
const RESERVED_FOLDERS: [&str; 2] = ["assets", "files"];
const FALLBACK_FOLDER: &str = "board";

#[must_use]
pub fn is_board_folder(folder: &Path) -> bool {
    folder.join(BOARD_FILE).is_file()
}

#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
    lock_deadline: Duration,
    volume: Option<Volume>,
}

impl Workspace {
    pub fn open(root: &Path) -> Result<Workspace, Error> {
        if !root.is_dir() {
            return Err(Error::WorkspaceNotFound {
                path: root.display().to_string(),
            });
        }
        Ok(Workspace {
            root: root.to_path_buf(),
            lock_deadline: DEFAULT_LOCK_DEADLINE,
            volume: None,
        })
    }

    #[must_use]
    pub fn with_lock_deadline(self, lock_deadline: Duration) -> Workspace {
        Workspace {
            lock_deadline,
            ..self
        }
    }

    #[cfg(any(test, feature = "volume-seam"))]
    #[must_use]
    pub fn assuming_volume(self, volume: Volume) -> Workspace {
        Workspace {
            volume: Some(volume),
            ..self
        }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn boards(&self) -> Result<Vec<BoardInfo>, Error> {
        let mut boards = Vec::new();
        collect_boards(&self.root, None, &mut boards)?;
        Ok(boards)
    }

    pub fn board(&self, id: &BoardId) -> Result<Board, Error> {
        let folder = self.folder_of(id);
        if !is_board_folder(&folder) {
            return Err(Error::BoardNotFound { board: id.clone() });
        }
        Ok(Board::at(
            id.clone(),
            BoardLocks::on_volume(&folder, self.volume),
            self.lock_deadline,
        ))
    }

    pub fn add_board(&self, name: &str, parent: Option<&BoardId>) -> Result<BoardInfo, Error> {
        let name = text::single_spaced(name);
        if name.is_empty() {
            return Err(Error::Invalid {
                problem: Invalid::EmptyBoardName,
            });
        }
        let parent_folder = match parent {
            Some(parent) => {
                self.board(parent)?;
                self.folder_of(parent)
            }
            None => self.root.clone(),
        };
        let (folder_name, folder) = create_free_folder(&parent_folder, &name)?;
        let path = folder.join(BOARD_FILE);
        let text = BoardFile::new(&name).render().map_err(malformed(&path))?;
        let held = BoardLocks::on_volume(&folder, self.volume)
            .acquire(&[LockKey::Config], self.lock_deadline)
            .map_err(lock_failure(&folder, "the new board"))?;
        create_new(&held, Target::BoardFile, text.as_bytes()).map_err(io_failure(&path))?;
        let id = child_id(parent, &folder_name).ok_or_else(|| Error::Io {
            path: folder.display().to_string(),
            message: "the new folder name is not a board id".to_owned(),
        })?;
        Ok(BoardInfo { id, name })
    }

    fn folder_of(&self, id: &BoardId) -> PathBuf {
        id.as_str()
            .split('/')
            .fold(self.root.clone(), |folder, part| folder.join(part))
    }
}

fn child_id(parent: Option<&BoardId>, folder_name: &str) -> Option<BoardId> {
    let id = match parent {
        Some(parent) => format!("{parent}/{folder_name}"),
        None => folder_name.to_owned(),
    };
    BoardId::parse(&id).ok()
}

fn create_free_folder(parent: &Path, name: &str) -> Result<(String, PathBuf), Error> {
    let base = Some(slug(name))
        .filter(|base| !base.is_empty() && !RESERVED_FOLDERS.contains(&base.as_str()))
        .unwrap_or_else(|| FALLBACK_FOLDER.to_owned());
    let mut attempt = 1_u32;
    loop {
        let candidate = if attempt == 1 {
            base.clone()
        } else {
            format!("{base}-{attempt}")
        };
        let folder = parent.join(&candidate);
        match fs::create_dir(&folder) {
            Ok(()) => return Ok((candidate, folder)),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => attempt += 1,
            Err(error) => return Err(io_failure(&folder)(error)),
        }
    }
}

fn collect_boards(
    folder: &Path,
    parent: Option<&BoardId>,
    boards: &mut Vec<BoardInfo>,
) -> Result<(), Error> {
    let mut names = Vec::new();
    for entry in fs::read_dir(folder).map_err(io_failure(folder))? {
        let entry = entry.map_err(io_failure(folder))?;
        if let Ok(name) = entry.file_name().into_string()
            && !RESERVED_FOLDERS.contains(&name.as_str())
            && is_board_folder(&entry.path())
        {
            names.push(name);
        }
    }
    names.sort();
    for name in names {
        let Some(id) = child_id(parent, &name) else {
            continue;
        };
        let inside = folder.join(&name);
        let title = read_board_file(&inside)
            .map(|file| file.name)
            .ok()
            .filter(|title| !title.is_empty())
            .unwrap_or(name);
        boards.push(BoardInfo {
            id: id.clone(),
            name: title,
        });
        collect_boards(&inside, Some(&id), boards)?;
    }
    Ok(())
}
