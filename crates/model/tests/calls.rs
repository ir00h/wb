use serde_json::{Value, json};
use wb_model::{
    ApplyOp, BoardId, BoardInfo, BoardSnapshot, Boards, CALLS, CallFailure, Error, Handler,
    NoteCard, NoteId, OpenBoard, Outcome, dispatch,
};

struct Garden;

impl Handler for Garden {
    fn list_boards(&self, (): ()) -> Result<Boards, Error> {
        Ok(Boards {
            boards: vec![BoardInfo {
                id: BoardId::parse("garden").expect("board id"),
                name: "Garden".to_owned(),
            }],
        })
    }

    fn open_board(&self, args: OpenBoard) -> Result<BoardSnapshot, Error> {
        Err(match args.board {
            Some(board) => Error::BoardNotFound { board },
            None => Error::NoBoardToOpen,
        })
    }

    fn apply(&self, args: ApplyOp) -> Result<Outcome, Error> {
        Ok(Outcome::Applied {
            note: NoteCard {
                id: NoteId::parse("n1").expect("note id"),
                title: args.board.to_string(),
                body: String::new(),
                tags: Vec::new(),
                section: None,
            },
        })
    }
}

fn failure(outcome: Result<Value, CallFailure>) -> Value {
    serde_json::to_value(outcome.expect_err("the call fails")).expect("serialised")
}

#[test]
fn every_call_of_the_table_reaches_its_handler() {
    assert_eq!(CALLS, ["listBoards", "openBoard", "apply"]);
    assert_eq!(
        dispatch(&Garden, "listBoards", Value::Null).expect("listed"),
        json!({ "boards": [{ "id": "garden", "name": "Garden" }] })
    );
    let applied = dispatch(
        &Garden,
        "apply",
        json!({ "board": "kitchen", "op": { "type": "editNote", "note": "n1", "edit": { "title": null, "body": { "type": "append", "text": "Basil" } } } }),
    )
    .expect("applied");
    assert_eq!(applied["type"], "applied");
    assert_eq!(applied["note"]["title"], "kitchen");
}

#[test]
fn a_failing_call_crosses_as_the_error_and_its_sentence() {
    assert_eq!(
        failure(dispatch(
            &Garden,
            "openBoard",
            json!({ "board": "kitchen" })
        )),
        json!({
            "error": { "type": "boardNotFound", "board": "kitchen" },
            "message": "there is no board `kitchen` in this workspace"
        })
    );
}

#[test]
fn unknown_calls_and_arguments_that_do_not_fit_are_refused() {
    let unknown = failure(dispatch(&Garden, "deleteEverything", Value::Null));
    assert_eq!(unknown["error"]["type"], "badCall");
    assert_eq!(
        unknown["error"]["message"],
        "there is no such call; the calls are listBoards, openBoard, apply"
    );
    let misfit = failure(dispatch(
        &Garden,
        "apply",
        json!({ "board": "Not a board" }),
    ));
    assert_eq!(misfit["error"]["type"], "badCall");
    assert_eq!(misfit["error"]["call"], "apply");
}
