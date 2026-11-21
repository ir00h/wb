use std::process::ExitCode;

use wb_model::Error;

const FALLBACK_SENTENCE: &str = "Something went wrong.";

#[derive(Debug)]
pub struct Failure(pub String);

pub type Result<T> = std::result::Result<T, Failure>;

impl Failure {
    pub fn report(self) -> ExitCode {
        eprintln!("{}", sentence(&self.0));
        ExitCode::FAILURE
    }
}

impl From<Error> for Failure {
    fn from(error: Error) -> Failure {
        Failure(error.to_string())
    }
}

macro_rules! fail {
    ($($arg:tt)*) => {
        return Err($crate::error::Failure(format!($($arg)*)))
    };
}
pub(crate) use fail;

pub fn sentence(text: &str) -> String {
    let text = text.trim();
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return FALLBACK_SENTENCE.to_owned();
    };
    let mut out: String = first.to_uppercase().chain(chars).collect();
    if !out.ends_with(['.', '?', '!']) {
        out.push('.');
    }
    out
}
