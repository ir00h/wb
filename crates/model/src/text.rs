use alloc::string::String;
use alloc::vec::Vec;

#[must_use]
pub fn is_space(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\n' | '\r')
}

#[must_use]
pub fn trim(text: &str) -> &str {
    text.trim_matches(is_space)
}

pub fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(is_space).filter(|word| !word.is_empty())
}

#[must_use]
pub fn single_spaced(text: &str) -> String {
    words(text).collect::<Vec<_>>().join(" ")
}
