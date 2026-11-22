use std::ffi::OsString;
use std::path::PathBuf;

use wb_model::BoardId;

pub const WORKSPACE_VARIABLE: &str = "WHITEBOARD_WORKSPACE";
pub const BOARD_VARIABLE: &str = "WB_BOARD";

#[derive(Debug, PartialEq, Eq)]
pub struct Settings {
    pub workspace: PathBuf,
    pub board: Option<BoardId>,
}

pub fn settings(
    arguments: impl IntoIterator<Item = OsString>,
    workspace_variable: Option<OsString>,
    board_variable: Option<OsString>,
) -> Result<Settings, String> {
    let mut workspace = None;
    let mut board = None;
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        let slot = match argument.to_str() {
            Some("--workspace") => &mut workspace,
            Some("--board") => &mut board,
            _ => {
                return Err(format!(
                    "`{}` is not an option; the options are --workspace <folder> and --board <board>",
                    argument.to_string_lossy()
                ));
            }
        };
        let Some(value) = arguments.next() else {
            return Err(format!(
                "{} needs a value after it",
                argument.to_string_lossy()
            ));
        };
        *slot = Some(value);
    }
    let workspace = workspace
        .or(workspace_variable)
        .filter(|folder| !folder.is_empty())
        .ok_or_else(|| {
            format!(
                "name the workspace folder with --workspace <folder> or the variable {WORKSPACE_VARIABLE}"
            )
        })?;
    let board = match board.or(board_variable).filter(|board| !board.is_empty()) {
        None => None,
        Some(board) => {
            let text = board.to_string_lossy();
            Some(
                BoardId::parse(&text)
                    .map_err(|error| format!("`{text}` is not a board: {error}"))?,
            )
        }
    };
    Ok(Settings {
        workspace: PathBuf::from(workspace),
        board,
    })
}

#[cfg(test)]
mod tests {
    use super::{Settings, settings};
    use std::ffi::OsString;
    use std::path::PathBuf;
    use wb_model::BoardId;

    fn words(text: &str) -> Vec<OsString> {
        text.split_whitespace().map(OsString::from).collect()
    }

    #[test]
    fn options_come_before_the_variables() {
        assert_eq!(
            settings(
                words("--workspace /boards --board kitchen/recipes"),
                Some("/elsewhere".into()),
                Some("garden".into())
            ),
            Ok(Settings {
                workspace: PathBuf::from("/boards"),
                board: Some(BoardId::parse("kitchen/recipes").expect("board id")),
            })
        );
        assert_eq!(
            settings(Vec::new(), Some("/boards".into()), None),
            Ok(Settings {
                workspace: PathBuf::from("/boards"),
                board: None,
            })
        );
    }

    #[test]
    fn a_missing_workspace_is_named_in_one_sentence() {
        assert_eq!(
            settings(words("--board garden"), None, None),
            Err(
                "name the workspace folder with --workspace <folder> or the variable WHITEBOARD_WORKSPACE"
                    .to_owned()
            )
        );
        assert!(settings(Vec::new(), Some(OsString::new()), None).is_err());
    }

    #[test]
    fn unknown_options_and_bad_boards_are_refused() {
        assert!(settings(words("--verbose"), Some("/boards".into()), None).is_err());
        assert!(settings(words("--board"), Some("/boards".into()), None).is_err());
        assert!(settings(words("--board Not/A/Board!"), Some("/boards".into()), None).is_err());
    }
}
