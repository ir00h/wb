use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::text;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("a tag needs at least one character besides spaces and `#`")]
pub struct EmptyTag;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(type = "string & { readonly __brand: \"Tag\" }")
)]
pub struct Tag(String);

impl Tag {
    #[must_use]
    pub fn new(text: &str) -> Option<Tag> {
        let name = text::words(text::trim(text).trim_start_matches('#'))
            .collect::<Vec<_>>()
            .join("-")
            .to_lowercase();
        (!name.is_empty()).then_some(Tag(name))
    }

    #[must_use]
    pub fn list<S: AsRef<str>>(items: &[S]) -> Vec<Tag> {
        let mut tags = Vec::new();
        for tag in items.iter().filter_map(|item| Tag::new(item.as_ref())) {
            if !tags.contains(&tag) {
                tags.push(tag);
            }
        }
        tags
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Tag {
    type Error = EmptyTag;

    fn try_from(text: String) -> Result<Tag, EmptyTag> {
        Tag::new(&text).ok_or(EmptyTag)
    }
}

impl From<Tag> for String {
    fn from(tag: Tag) -> String {
        tag.0
    }
}

impl AsRef<str> for Tag {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}
