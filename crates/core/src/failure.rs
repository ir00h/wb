use std::io;
use std::path::Path;

use wb_model::Error;

use crate::lock::LockError;

pub fn io_failure(path: &Path) -> impl Fn(io::Error) -> Error {
    move |error| Error::Io {
        path: path.display().to_string(),
        message: error.to_string(),
    }
}

pub fn malformed(path: &Path) -> impl Fn(serde_json::Error) -> Error {
    move |error| Error::Malformed {
        path: path.display().to_string(),
        message: error.to_string(),
    }
}

pub fn not_text(path: &Path) -> impl Fn(std::str::Utf8Error) -> Error {
    move |error| Error::Malformed {
        path: path.display().to_string(),
        message: error.to_string(),
    }
}

pub fn kept_changing(path: &Path) -> Error {
    Error::Io {
        path: path.display().to_string(),
        message: "another program kept changing the file, so nothing was written; try again"
            .to_owned(),
    }
}

pub fn lock_failure(folder: &Path, what: &str) -> impl Fn(LockError) -> Error {
    move |error| match error {
        LockError::Busy { waited, .. } => Error::Busy {
            what: what.to_owned(),
            waited_ms: u64::try_from(waited.as_millis()).unwrap_or(u64::MAX),
        },
        LockError::Unsupported { path } => Error::LocksUnsupported {
            path: path.display().to_string(),
        },
        LockError::Io(error) => io_failure(folder)(error),
    }
}
