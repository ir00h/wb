use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use wb_core::Board;
use wb_model::{BoardSnapshot, BoardUpdate, Error, diff};

const QUIET: Duration = Duration::from_millis(120);
const TEMPORARY_SUFFIXES: [&str; 4] = ["~", ".tmp", ".swp", ".swx"];

pub trait Publish: Send + Sync + 'static {
    fn publish(&self, update: BoardUpdate);
}

struct Watched {
    board: Board,
    sent: BoardSnapshot,
    _watcher: RecommendedWatcher,
}

pub struct BoardWatch {
    watched: Arc<Mutex<Option<Watched>>>,
    changed: Sender<()>,
}

impl BoardWatch {
    pub fn start(publisher: impl Publish) -> BoardWatch {
        let watched: Arc<Mutex<Option<Watched>>> = Arc::default();
        let (changed, signals) = mpsc::channel();
        let shared = Arc::clone(&watched);
        std::thread::spawn(move || follow(&signals, &shared, &publisher));
        BoardWatch { watched, changed }
    }

    pub fn open(&self, board: Board) -> Result<BoardSnapshot, Error> {
        let mut current = self.watched.lock().unwrap_or_else(PoisonError::into_inner);
        *current = None;
        let folder = canonical(board.folder());
        let changed = self.changed.clone();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            if event.is_ok_and(|event| event.paths.iter().any(|path| concerns(&folder, path))) {
                let _ = changed.send(());
            }
        })
        .map_err(|error| cannot_watch(board.folder(), &error))?;
        watcher
            .watch(board.folder(), RecursiveMode::NonRecursive)
            .map_err(|error| cannot_watch(board.folder(), &error))?;
        let sent = board.snapshot()?;
        *current = Some(Watched {
            board,
            sent: sent.clone(),
            _watcher: watcher,
        });
        Ok(sent)
    }
}

fn cannot_watch(folder: &Path, error: &notify::Error) -> Error {
    Error::Io {
        path: folder.display().to_string(),
        message: format!("the board's folder cannot be watched for changes: {error}"),
    }
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn concerns(folder: &Path, path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if name.starts_with('.')
        || TEMPORARY_SUFFIXES
            .iter()
            .any(|suffix| name.ends_with(suffix))
    {
        return false;
    }
    path.parent()
        .is_some_and(|parent| canonical(parent) == folder)
}

fn follow(signals: &Receiver<()>, watched: &Mutex<Option<Watched>>, publisher: &impl Publish) {
    while signals.recv().is_ok() {
        loop {
            match signals.recv_timeout(QUIET) {
                Ok(()) => {}
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        let mut watched = watched.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(watched) = watched.as_mut() else {
            continue;
        };
        let Ok(now) = watched.board.snapshot() else {
            continue;
        };
        let events = diff(&watched.sent, &now);
        watched.sent = now;
        if !events.is_empty() {
            publisher.publish(BoardUpdate {
                board: watched.board.id().clone(),
                events,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::concerns;
    use std::path::Path;

    #[test]
    fn only_the_board_folders_own_visible_files_count() {
        let folder = Path::new("/no/such/garden");
        assert!(concerns(folder, &folder.join("plant-list-n1.md")));
        assert!(concerns(folder, &folder.join("board.config.json")));
        assert!(!concerns(folder, &folder.join(".locks")));
        assert!(!concerns(folder, &folder.join(".trash")));
        assert!(!concerns(folder, &folder.join("plant-list-n1.md~")));
        assert!(!concerns(folder, &folder.join("board.config.tmp")));
        assert!(!concerns(folder, &folder.join("seeds").join("bean-n2.md")));
        assert!(!concerns(
            folder,
            &folder.join(".trash").join("plant-list-n1.md")
        ));
    }
}
