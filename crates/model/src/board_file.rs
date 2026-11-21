use alloc::borrow::ToOwned;
use alloc::string::String;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const BOARD_FILE: &str = "board.json";
pub const BOARD_FORMAT_VERSION: u32 = 2;
const CARDS_KEY: &str = "cards";
const SECTIONS_KEY: &str = "sections";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoardFile {
    pub version: u32,
    pub name: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl BoardFile {
    #[must_use]
    pub fn new(name: &str) -> BoardFile {
        let mut extra = Map::new();
        for key in [CARDS_KEY, SECTIONS_KEY] {
            extra.insert(key.to_owned(), Value::Object(Map::new()));
        }
        BoardFile {
            version: BOARD_FORMAT_VERSION,
            name: name.to_owned(),
            extra,
        }
    }

    pub fn parse(text: &str) -> Result<BoardFile, serde_json::Error> {
        serde_json::from_str(text)
    }

    pub fn render(&self) -> Result<String, serde_json::Error> {
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        Ok(text)
    }

    pub fn card_ids(&self) -> impl Iterator<Item = &str> {
        self.keys_of(CARDS_KEY)
    }

    pub fn section_ids(&self) -> impl Iterator<Item = &str> {
        self.keys_of(SECTIONS_KEY)
    }

    fn keys_of(&self, key: &str) -> impl Iterator<Item = &str> {
        self.extra
            .get(key)
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|entries| entries.keys().map(String::as_str))
    }
}
