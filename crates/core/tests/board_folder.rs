use std::fs;
use std::path::PathBuf;

fn scratch_folder(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("wb-core-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).expect("scratch folder");
    folder
}

#[test]
fn a_folder_with_a_board_file_is_a_board() {
    let folder = scratch_folder("board");
    fs::write(folder.join(wb_core::BOARD_FILE), "{}").expect("board file");
    assert!(wb_core::is_board_folder(&folder));
    fs::remove_dir_all(folder).expect("cleanup");
}

#[test]
fn a_folder_without_a_board_file_is_not_a_board() {
    let folder = scratch_folder("plain");
    assert!(!wb_core::is_board_folder(&folder));
    fs::remove_dir_all(folder).expect("cleanup");
}
