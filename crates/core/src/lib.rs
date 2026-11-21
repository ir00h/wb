use std::path::Path;

pub const BOARD_FILE: &str = "board.json";

#[must_use]
pub fn is_board_folder(folder: &Path) -> bool {
    folder.join(BOARD_FILE).is_file()
}
