use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::thread;
use std::time::Duration;

use wb_model::{BoardConfig, CONFIG_FILE, Error};

use crate::failure::{io_failure, kept_changing, malformed, not_a_plain_file, not_text};
use crate::lock::Held;
use crate::replace::{Created, Replaced, Target, create_new, replace_if_unchanged};

pub const ATTEMPTS: usize = 16;
const LOOKS_AT_A_BLANK_FILE: usize = 6;
const PAUSE_BEFORE_ANOTHER_LOOK: Duration = Duration::from_millis(25);

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

fn is_blank(bytes: &[u8]) -> bool {
    bytes.iter().all(u8::is_ascii_whitespace)
}

fn read_when_settled(path: &Path) -> Result<Option<Vec<u8>>, Error> {
    let mut bytes = read_bytes(path)?;
    for _ in 1..LOOKS_AT_A_BLANK_FILE {
        if !bytes.as_deref().is_some_and(is_blank) {
            break;
        }
        thread::sleep(PAUSE_BEFORE_ANOTHER_LOOK);
        bytes = read_bytes(path)?;
    }
    Ok(bytes)
}

pub fn read_for_a_write(board: &Path) -> Result<BoardConfig, Error> {
    let path = board.join(CONFIG_FILE);
    parse(&path, read_when_settled(&path)?.as_deref())
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
    let mut on_disk = read_when_settled(&path)?;
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
                Created::Exists if is_there_but_not_a_plain_file(&path) => Replaced::NotAPlainFile,
                Created::Exists => Replaced::Gone,
            },
        };
        on_disk = match replaced {
            Replaced::Done => return Ok(answer),
            Replaced::NotAPlainFile => return Err(not_a_plain_file(&path)),
            Replaced::Changed { now } if !is_blank(&now) => Some(now),
            Replaced::Changed { .. } | Replaced::Gone => read_when_settled(&path)?,
        };
    }
    Err(kept_changing(&path))
}

fn is_there_but_not_a_plain_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|entry| !entry.file_type().is_file())
}
