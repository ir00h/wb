use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::text;

pub const SECTION_SEPARATOR: &str = " / ";
const SEPARATOR_WORD: &str = "/";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("a section path names at least one section")]
pub struct EmptySectionPath;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(type = "string"))]
pub struct SectionPath(Vec<String>);

impl SectionPath {
    #[must_use]
    pub fn parse(text: &str) -> Option<SectionPath> {
        let words: Vec<&str> = text::words(text).collect();
        let names: Vec<String> = words
            .split(|word| *word == SEPARATOR_WORD)
            .map(|name| name.join(" "))
            .filter(|name| !name.is_empty())
            .collect();
        (!names.is_empty()).then_some(SectionPath(names))
    }

    #[must_use]
    pub fn from_names<S: AsRef<str>>(names: &[S]) -> Option<SectionPath> {
        let names: Vec<String> = names
            .iter()
            .map(|name| section_name(name.as_ref()))
            .filter(|name| !name.is_empty())
            .collect();
        (!names.is_empty()).then_some(SectionPath(names))
    }

    #[must_use]
    pub fn names(&self) -> &[String] {
        &self.0
    }
}

#[must_use]
pub fn section_name(name: &str) -> String {
    let mut written = String::new();
    let mut glued = false;
    for word in text::words(name) {
        if word == SEPARATOR_WORD {
            written.push_str(SEPARATOR_WORD);
            glued = true;
            continue;
        }
        if !written.is_empty() && !glued {
            written.push(' ');
        }
        written.push_str(word);
        glued = false;
    }
    written
}

#[must_use]
pub fn same_section_name(one: &str, other: &str) -> bool {
    section_name(one).to_lowercase() == section_name(other).to_lowercase()
}

impl TryFrom<String> for SectionPath {
    type Error = EmptySectionPath;

    fn try_from(text: String) -> Result<SectionPath, EmptySectionPath> {
        SectionPath::parse(&text).ok_or(EmptySectionPath)
    }
}

impl From<SectionPath> for String {
    fn from(path: SectionPath) -> String {
        path.to_string()
    }
}

impl fmt::Display for SectionPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0.join(SECTION_SEPARATOR))
    }
}
