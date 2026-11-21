use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use wb_model::{BoardConfig, CONFIG_FILE, Error};

use crate::failure::{io_failure, kept_changing, malformed, not_text};
use crate::lock::Held;
use crate::replace::{Created, Replaced, Target, create_new, replace_if_unchanged};

pub const ATTEMPTS: usize = 16;

fn read_bytes(path: &Path) -> Result<Option<Vec<u8>>, Error> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_failure(path)(error)),
    }
}

fn parse(path: &Path, bytes: Option<&[u8]>) -> Result<BoardConfig, Error> {
    let Some(bytes) = bytes else {
        return Ok(BoardConfig::default());
    };
    let text = std::str::from_utf8(bytes).map_err(not_text(path))?;
    BoardConfig::parse(text).map_err(malformed(path))
}

pub fn read(board: &Path) -> Result<BoardConfig, Error> {
    let path = board.join(CONFIG_FILE);
    parse(&path, read_bytes(&path)?.as_deref())
}

pub fn change<T>(
    held: &Held,
    mut edit: impl FnMut(&mut BoardConfig) -> Result<T, Error>,
) -> Result<T, Error> {
    let path = held.board().join(CONFIG_FILE);
    let mut on_disk = read_bytes(&path)?;
    for _ in 0..ATTEMPTS {
        let mut config = parse(&path, on_disk.as_deref())?;
        let answer = edit(&mut config)?;
        let text = config.render().map_err(malformed(&path))?;
        let replaced = match &on_disk {
            Some(read) if read == text.as_bytes() => Replaced::Done,
            Some(read) => replace_if_unchanged(held, Target::Config, read, text.as_bytes())
                .map_err(io_failure(&path))?,
            None => match create_new(held, Target::Config, text.as_bytes())
                .map_err(io_failure(&path))?
            {
                Created::Done => Replaced::Done,
                Created::Exists => Replaced::Gone,
            },
        };
        on_disk = match replaced {
            Replaced::Done => return Ok(answer),
            Replaced::Changed { now } => Some(now),
            Replaced::Gone => read_bytes(&path)?,
        };
    }
    Err(kept_changing(&path))
}
