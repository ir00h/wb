use std::env;
use std::ffi::OsString;
use std::path::PathBuf;

use wb_core::{Board, Workspace};
use wb_model::{BoardId, Error, SectionTree};

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

impl BoardRef {
    pub fn sections(&self) -> Result<SectionTree> {
        Ok(SectionTree::from_config(&self.board.config()?))
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
