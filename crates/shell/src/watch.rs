use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use wb_core::Board;
use wb_model::{BoardHealth, BoardSnapshot, BoardUpdate, Error, diff};

const QUIET: Duration = Duration::from_millis(120);
const LONGEST_WAIT: Duration = Duration::from_millis(500);
const RECHECK: Duration = Duration::from_secs(2);
const TEMPORARY_SUFFIXES: [&str; 4] = ["~", ".tmp", ".swp", ".swx"];

pub trait Publish: Send + Sync + 'static {
    fn publish(&self, update: BoardUpdate);
    fn health(&self, health: BoardHealth);
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Fingerprint(Option<Vec<(OsString, u64, Option<SystemTime>)>>);

#[derive(Clone, Copy)]
enum Cause {
    Reported,
    Recheck,
}

struct Watched {
    board: Board,
    folder: PathBuf,
    sent: BoardSnapshot,
    seen: Fingerprint,
    problem: Option<String>,
}

pub struct BoardWatch {
    watched: Arc<Mutex<Option<Watched>>>,
    watcher: Mutex<Option<RecommendedWatcher>>,
    changed: Sender<()>,
}

impl BoardWatch {
    pub fn start(publisher: impl Publish) -> BoardWatch {
        let watched: Arc<Mutex<Option<Watched>>> = Arc::default();
        let (changed, signals) = mpsc::channel();
        let shared = Arc::clone(&watched);
        std::thread::spawn(move || follow(&signals, &shared, &publisher));
        BoardWatch {
            watched,
            watcher: Mutex::default(),
            changed,
        }
    }

    pub fn open(&self, board: Board) -> Result<BoardSnapshot, Error> {
        let mut current = self.watched.lock().unwrap_or_else(PoisonError::into_inner);
        *current = None;
        let folder = canonical(board.folder());
        let mut watcher = self.watcher.lock().unwrap_or_else(PoisonError::into_inner);
        *watcher = Some(arm(&folder, &self.changed)?);
        let seen = fingerprint(&folder);
        let sent = board.snapshot()?;
        *current = Some(Watched {
            board,
            folder,
            sent: sent.clone(),
            seen,
            problem: None,
        });
        Ok(sent)
    }
}

fn arm(folder: &Path, changed: &Sender<()>) -> Result<RecommendedWatcher, Error> {
    let board_folder = folder.to_path_buf();
    let changed = changed.clone();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
        if event.is_ok_and(|event| event.paths.iter().any(|path| concerns(&board_folder, path))) {
            let _ = changed.send(());
        }
    })
    .map_err(|error| cannot_watch(folder, &error))?;
    watcher
        .watch(folder, RecursiveMode::NonRecursive)
        .map_err(|error| cannot_watch(folder, &error))?;
    if let Some(parent) = folder.parent() {
        watcher
            .watch(parent, RecursiveMode::NonRecursive)
            .map_err(|error| cannot_watch(parent, &error))?;
    }
    Ok(watcher)
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

fn counts(name: &str) -> bool {
    !name.starts_with('.')
        && !TEMPORARY_SUFFIXES
            .iter()
            .any(|suffix| name.ends_with(suffix))
}

fn concerns(folder: &Path, path: &Path) -> bool {
    if path == folder {
        return true;
    }
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    counts(name)
        && path
            .parent()
            .is_some_and(|parent| canonical(parent) == folder)
}

fn fingerprint(folder: &Path) -> Fingerprint {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Fingerprint(None);
    };
    let mut seen: Vec<(OsString, u64, Option<SystemTime>)> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_str().is_some_and(counts))
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            Some((entry.file_name(), metadata.len(), metadata.modified().ok()))
        })
        .collect();
    seen.sort();
    Fingerprint(Some(seen))
}

fn settle(signals: &Receiver<()>) -> Result<(), RecvTimeoutError> {
    let deadline = Instant::now() + LONGEST_WAIT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Ok(());
        }
        match signals.recv_timeout(QUIET.min(left)) {
            Ok(()) => {}
            Err(RecvTimeoutError::Timeout) => return Ok(()),
            Err(disconnected) => return Err(disconnected),
        }
    }
}

fn follow(signals: &Receiver<()>, watched: &Mutex<Option<Watched>>, publisher: &impl Publish) {
    loop {
        let cause = match signals.recv_timeout(RECHECK) {
            Ok(()) => {
                if settle(signals).is_err() {
                    return;
                }
                Cause::Reported
            }
            Err(RecvTimeoutError::Timeout) => Cause::Recheck,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        let mut watched = watched.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(watched) = watched.as_mut() {
            refresh(watched, cause, publisher);
        }
    }
}

fn refresh(watched: &mut Watched, cause: Cause, publisher: &impl Publish) {
    let seen = fingerprint(&watched.folder);
    if matches!(cause, Cause::Recheck) && seen == watched.seen && watched.problem.is_none() {
        return;
    }
    watched.seen = seen;
    let board = watched.board.id().clone();
    match watched.board.snapshot() {
        Ok(now) => {
            if watched.problem.take().is_some() {
                publisher.health(BoardHealth {
                    board: board.clone(),
                    problem: None,
                });
            }
            let events = diff(&watched.sent, &now);
            watched.sent = now;
            if !events.is_empty() {
                publisher.publish(BoardUpdate { board, events });
            }
        }
        Err(error) => {
            let problem = error.to_string();
            if watched.problem.as_ref() != Some(&problem) {
                watched.problem = Some(problem.clone());
                publisher.health(BoardHealth {
                    board,
                    problem: Some(problem),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Fingerprint, Publish, RECHECK, Watched, concerns, fingerprint, follow};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex, PoisonError, mpsc};
    use std::time::{Duration, Instant};
    use wb_core::Workspace;
    use wb_model::{BoardEvent, BoardHealth, BoardUpdate};

    #[derive(Clone, Default)]
    struct Recorder(Arc<Mutex<Vec<String>>>);

    impl Recorder {
        fn seen(&self) -> Vec<String> {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }

        fn wait_for(&self, count: usize) -> Vec<String> {
            let deadline = Instant::now() + RECHECK * 2 + Duration::from_secs(1);
            while Instant::now() < deadline && self.seen().len() < count {
                std::thread::sleep(Duration::from_millis(50));
            }
            self.seen()
        }
    }

    impl Publish for Recorder {
        fn publish(&self, update: BoardUpdate) {
            let mut seen = self.0.lock().unwrap_or_else(PoisonError::into_inner);
            for event in update.events {
                if let BoardEvent::NoteChanged { note } = event {
                    seen.push(format!("note {}", note.title));
                }
            }
        }

        fn health(&self, health: BoardHealth) {
            let mut seen = self.0.lock().unwrap_or_else(PoisonError::into_inner);
            seen.push(match health.problem {
                Some(problem) => format!("unreadable: {problem}"),
                None => "readable".to_owned(),
            });
        }
    }

    fn fingerprint_unchanged(watched: &Mutex<Option<Watched>>) -> bool {
        let watched = watched.lock().unwrap_or_else(PoisonError::into_inner);
        let watched = watched.as_ref().expect("a board is watched");
        fingerprint(&watched.folder) == watched.seen
    }

    fn scratch_workspace() -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "whiteboard-watch-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("scratch folder");
        folder
    }

    #[test]
    fn the_recheck_catches_changes_no_event_reported() {
        let root = scratch_workspace();
        let workspace = Workspace::open(&root).expect("workspace");
        let info = workspace.add_board("Garden", None).expect("board");
        let board = workspace.board(&info.id).expect("board opens");
        let folder = board.folder().canonicalize().expect("folder");
        let sent = board.snapshot().expect("snapshot");
        let watched = Arc::new(Mutex::new(Some(Watched {
            board,
            seen: fingerprint(&folder),
            folder: folder.clone(),
            sent,
            problem: None,
        })));
        let recorder = Recorder::default();
        let (quiet, signals) = mpsc::channel();
        let following = {
            let watched = Arc::clone(&watched);
            let recorder = recorder.clone();
            std::thread::spawn(move || follow(&signals, &watched, &recorder))
        };

        std::fs::write(
            folder.join("plant-list-n1.md"),
            "---\nid: n1\n---\n\n# Plant list\n\nTomatoes.\n",
        )
        .expect("note written");
        assert_eq!(recorder.wait_for(1), ["note Plant list"]);

        let config = folder.join("board.config.json");
        std::fs::write(&config, "{ not json").expect("config broken");
        let seen = recorder.wait_for(2);
        assert_eq!(seen.len(), 2, "{seen:?}");
        assert!(seen[1].starts_with("unreadable: "), "{seen:?}");

        std::fs::write(&config, "[]").expect("config broken another way");
        let seen = recorder.wait_for(3);
        assert_eq!(seen.len(), 3, "a second cause was not published: {seen:?}");
        assert!(seen[2].starts_with("unreadable: "), "{seen:?}");
        assert_ne!(seen[1], seen[2]);

        let broken_at = std::fs::metadata(&config)
            .and_then(|metadata| metadata.modified())
            .expect("config time");
        std::thread::sleep(RECHECK + Duration::from_millis(500));
        let repair = folder.join("board.config.tmp");
        std::fs::write(&repair, "{}").expect("repair written");
        std::fs::File::options()
            .write(true)
            .open(&repair)
            .and_then(|file| file.set_modified(broken_at))
            .expect("config time kept");
        std::fs::rename(&repair, &config).expect("config repaired");
        assert!(fingerprint_unchanged(&watched));
        assert_eq!(recorder.wait_for(4)[3..], ["readable"]);

        drop(quiet);
        following.join().expect("follow ends");
        std::fs::remove_dir_all(&root).expect("scratch removed");
    }

    #[test]
    fn only_the_board_folder_and_its_own_visible_files_count() {
        let folder = Path::new("/no/such/garden");
        assert!(concerns(folder, folder));
        assert!(concerns(folder, &folder.join("plant-list-n1.md")));
        assert!(concerns(folder, &folder.join("board.config.json")));
        assert!(!concerns(folder, Path::new("/no/such/kitchen")));
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

    #[test]
    fn a_folder_that_is_gone_has_no_fingerprint() {
        assert_eq!(fingerprint(Path::new("/no/such/garden")), Fingerprint(None));
    }
}
