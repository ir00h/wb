use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::any::TypeId;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::Error;
use crate::id::BoardId;
use crate::wire::{BoardEvent, BoardInfo, BoardSnapshot, Op, Outcome};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Boards {
    pub boards: Vec<BoardInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct OpenBoard {
    pub board: Option<BoardId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ApplyOp {
    pub board: BoardId,
    pub op: Op,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BoardUpdate {
    pub board: BoardId,
    pub events: Vec<BoardEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BoardHealth {
    pub board: BoardId,
    pub problem: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum CloseAnswer {
    Leave,
    Stay,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CallFailure {
    pub error: Error,
    pub message: String,
}

impl From<Error> for CallFailure {
    fn from(error: Error) -> CallFailure {
        CallFailure {
            message: error.to_string(),
            error,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TitleBar {
    pub height: f64,
    pub buttons_x: f64,
    pub buttons_y: f64,
    pub content_inset: f64,
}

pub const TITLE_BAR: TitleBar = TitleBar {
    height: 48.0,
    buttons_x: 18.0,
    buttons_y: 26.0,
    content_inset: 94.0,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallShape {
    pub name: &'static str,
    pub args: TypeId,
    pub reply: TypeId,
}

fn bad_call(call: &str, message: &impl ToString) -> CallFailure {
    CallFailure::from(Error::BadCall {
        call: call.to_string(),
        message: message.to_string(),
    })
}

macro_rules! call_table {
    (
        calls { $( $method:ident = $wire:literal ( $args:ty ) -> $reply:ty ; )* }
        events { $( $event:ident = $event_wire:literal ( $payload:ty ) ; )* }
    ) => {
        pub trait Handler {
            $( fn $method(&self, args: $args) -> Result<$reply, Error>; )*
        }

        pub const CALLS: &[&str] = &[ $( $wire ),* ];
        $( pub const $event: &str = $event_wire; )*

        pub fn dispatch<H: Handler + ?Sized>(
            handler: &H,
            call: &str,
            args: Value,
        ) -> Result<Value, CallFailure> {
            match call {
                $(
                    $wire => {
                        let args: $args = serde_json::from_value(args)
                            .map_err(|error| bad_call(call, &error))?;
                        let reply = handler.$method(args)?;
                        serde_json::to_value(reply).map_err(|error| bad_call(call, &error))
                    }
                )*
                _ => Err(bad_call(
                    call,
                    &format!("there is no such call; the calls are {}", CALLS.join(", ")),
                )),
            }
        }

        #[must_use]
        pub fn call_shapes() -> Vec<CallShape> {
            alloc::vec![
                $( CallShape { name: $wire, args: TypeId::of::<$args>(), reply: TypeId::of::<$reply>() } ),*
            ]
        }

        #[must_use]
        pub fn event_shapes() -> Vec<CallShape> {
            alloc::vec![
                $( CallShape { name: $event_wire, args: TypeId::of::<$payload>(), reply: TypeId::of::<()>() } ),*
            ]
        }
    };
}

call_table! {
    calls {
        list_boards = "listBoards" (()) -> Boards;
        open_board = "openBoard" (OpenBoard) -> BoardSnapshot;
        apply = "apply" (ApplyOp) -> Outcome;
        answer_close = "answerClose" (CloseAnswer) -> ();
    }
    events {
        BOARD_EVENT = "board" (BoardUpdate);
        BOARD_HEALTH_EVENT = "boardHealth" (BoardHealth);
        CLOSE_REQUESTED_EVENT = "closeRequested" (());
    }
}
