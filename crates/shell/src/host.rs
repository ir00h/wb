use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tauri::{AppHandle, Emitter, State};
use wb_core::Workspace;
use wb_model::{
    ApplyOp, BOARD_EVENT, BoardId, BoardSnapshot, BoardUpdate, Boards, CallFailure, CloseAnswer,
    Error, Handler, OpenBoard, Outcome, dispatch,
};

use crate::closing::Closing;
use crate::watch::{BoardWatch, Publish};

const LOCK_DEADLINE: Duration = Duration::from_secs(2);

pub struct Host {
    workspace: Workspace,
    first_board: Option<BoardId>,
    watch: BoardWatch,
    closing: Arc<Closing>,
}

struct Window(AppHandle);

impl Publish for Window {
    fn publish(&self, update: BoardUpdate) {
        let _ = self.0.emit(BOARD_EVENT, update);
    }
}

impl Host {
    pub fn new(
        workspace: Workspace,
        first_board: Option<BoardId>,
        app: AppHandle,
        closing: Arc<Closing>,
    ) -> Host {
        Host {
            workspace: workspace.with_lock_deadline(LOCK_DEADLINE),
            first_board,
            watch: BoardWatch::start(Window(app)),
            closing,
        }
    }
}

impl Handler for Host {
    fn list_boards(&self, (): ()) -> Result<Boards, Error> {
        Ok(Boards {
            boards: self.workspace.boards()?,
        })
    }

    fn open_board(&self, args: OpenBoard) -> Result<BoardSnapshot, Error> {
        let id = match args.board.or_else(|| self.first_board.clone()) {
            Some(id) => id,
            None => self
                .workspace
                .boards()?
                .into_iter()
                .next()
                .map(|board| board.id)
                .ok_or(Error::NoBoardToOpen)?,
        };
        self.watch.open(self.workspace.board(&id)?)
    }

    fn apply(&self, args: ApplyOp) -> Result<Outcome, Error> {
        let _in_flight = self.closing.operation();
        #[cfg(feature = "automation")]
        let applied = crate::automation::Applied::of(&args.op);
        let outcome = self.workspace.board(&args.board)?.apply(args.op)?;
        #[cfg(feature = "automation")]
        applied.record();
        Ok(outcome)
    }

    fn answer_close(&self, answer: CloseAnswer) -> Result<(), Error> {
        self.closing.answer(answer);
        Ok(())
    }
}

#[tauri::command]
pub async fn call(
    host: State<'_, Arc<Host>>,
    call: String,
    args: Value,
) -> Result<Value, CallFailure> {
    let host = Arc::clone(&host);
    tauri::async_runtime::spawn_blocking(move || dispatch(host.as_ref(), &call, args))
        .await
        .map_err(|error| {
            CallFailure::from(Error::Io {
                path: String::new(),
                message: format!("the call stopped before it answered: {error}"),
            })
        })?
}
