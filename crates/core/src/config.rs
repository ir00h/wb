use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use wb_model::{BoardConfig, CONFIG_FILE, Error};

use crate::failure::{io_failure, malformed};
use crate::lock::{Held, LockKey, replace_file};

fn read_text(board: &Path) -> Result<Option<String>, Error> {
    let path = board.join(CONFIG_FILE);
    match fs::read_to_string(&path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_failure(&path)(error)),
    }
}

pub fn read(board: &Path) -> Result<BoardConfig, Error> {
    let path = board.join(CONFIG_FILE);
    match read_text(board)? {
        Some(text) => BoardConfig::parse(&text).map_err(malformed(&path)),
        None => Ok(BoardConfig::default()),
    }
}

pub fn write(board: &Path, config: &BoardConfig, held: &Held) -> Result<(), Error> {
    debug_assert!(held.covers(&LockKey::Config));
    let path = board.join(CONFIG_FILE);
    let text = config.render().map_err(malformed(&path))?;
    if read_text(board)?.as_deref() == Some(text.as_str()) {
        return Ok(());
    }
    replace_file(&path, text.as_bytes()).map_err(io_failure(&path))
}
