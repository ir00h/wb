use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use wb_core::{Board, Workspace};
use wb_model::{BOARD_FILE, BoardConfig, BoardFile, BoardId, CONFIG_FILE, Error, SectionTree};

use crate::error::{Failure, Result, fail};

pub const WORKSPACE_ENV: &str = "WHITEBOARD_WORKSPACE";
pub const BOARD_ENV: &str = "WB_BOARD";
const BOARD_ID_EXAMPLE: &str = "kitchen/recipes";

pub struct Context {
    pub root: PathBuf,
    pub json: bool,
    board: Option<String>,
}

pub struct BoardRef {
    pub id: BoardId,
    pub board: Board,
}

fn read_text(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Failure::from(Error::Io {
            path: path.display().to_string(),
            message: error.to_string(),
        })),
    }
}

fn malformed(path: &Path) -> impl Fn(serde_json::Error) -> Failure {
    move |error| {
        Failure::from(Error::Malformed {
            path: path.display().to_string(),
            message: error.to_string(),
        })
    }
}

impl BoardRef {
    pub fn name(&self) -> Result<String> {
        let path = self.board.folder().join(BOARD_FILE);
        let Some(text) = read_text(&path)? else {
            return Err(Failure::from(Error::BoardNotFound {
                board: self.id.clone(),
            }));
        };
        Ok(BoardFile::parse(&text).map_err(malformed(&path))?.name)
    }

    pub fn config(&self) -> Result<BoardConfig> {
        let path = self.board.folder().join(CONFIG_FILE);
        match read_text(&path)? {
            Some(text) => BoardConfig::parse(&text).map_err(malformed(&path)),
            None => Ok(BoardConfig::default()),
        }
    }

    pub fn sections(&self) -> Result<SectionTree> {
        Ok(SectionTree::from_config(&self.config()?))
    }
}

fn given(value: Option<OsString>) -> Option<OsString> {
    value.filter(|value| !value.is_empty())
}

impl Context {
    pub fn new(workspace: Option<PathBuf>, board: Option<String>, json: bool) -> Result<Context> {
        let Some(workspace) =
            given(workspace.map(OsString::from)).or_else(|| given(env::var_os(WORKSPACE_ENV)))
        else {
            fail!("no workspace given: pass --workspace <path> or set {WORKSPACE_ENV}")
        };
        let root = match env::current_dir() {
            Ok(cwd) => cwd.join(workspace),
            Err(_) => PathBuf::from(workspace),
        };
        let board = board
            .or_else(|| env::var(BOARD_ENV).ok())
            .map(|board| board.trim().trim_matches('/').to_owned())
            .filter(|board| !board.is_empty());
        Ok(Context { root, json, board })
    }

    pub fn workspace(&self) -> Result<Workspace> {
        Workspace::open(&self.root).map_err(|error| match error {
            Error::WorkspaceNotFound { .. } => Failure(format!(
                "there is no workspace at {}; pass --workspace, set {WORKSPACE_ENV}, or create a board with `wb board add`",
                self.root.display()
            )),
            other => Failure::from(other),
        })
    }

    pub fn board_named(&self, text: &str) -> Result<BoardRef> {
        let workspace = self.workspace()?;
        let text = text.trim().trim_matches('/');
        let Ok(id) = BoardId::parse(text) else {
            fail!(
                "\"{text}\" is not a board id; ids are folder paths like {BOARD_ID_EXAMPLE}{}",
                known_boards(&workspace)
            )
        };
        let board = workspace.board(&id).map_err(|error| match error {
            Error::BoardNotFound { .. } => Failure(format!(
                "there is no board {id}{}",
                known_boards(&workspace)
            )),
            other => Failure::from(other),
        })?;
        Ok(BoardRef { id, board })
    }

    pub fn board(&self) -> Result<BoardRef> {
        if let Some(id) = &self.board {
            return self.board_named(id);
        }
        let workspace = self.workspace()?;
        fail!(
            "no board given: pass --board <id> or set {BOARD_ENV}{}",
            known_boards(&workspace)
        )
    }
}

fn known_boards(workspace: &Workspace) -> String {
    let ids: Vec<String> = workspace
        .boards()
        .unwrap_or_default()
        .into_iter()
        .map(|board| board.id.to_string())
        .collect();
    if ids.is_empty() {
        "; the workspace has no boards yet".to_owned()
    } else {
        format!("; boards: {}", ids.join(", "))
    }
}
