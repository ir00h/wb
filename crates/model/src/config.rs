use alloc::borrow::ToOwned;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};

use crate::error::Invalid;
use crate::id::SectionId;
use crate::section_path::{same_section_name, section_name};
use crate::text;

pub const CONFIG_FILE: &str = "board.config.json";
pub const CONFIG_VERSION: u32 = 1;
pub const HUE_MIN: f64 = 0.0;
pub const HUE_MAX: f64 = 360.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardConfig {
    #[serde(default = "current_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<TagColour>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sections: Option<Vec<SectionEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_section: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive_section: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repos: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hue: Option<Number>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requests: Option<Vec<Value>>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TagColour {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hue: Option<Number>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionEntry {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hue: Option<Number>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn written_hue(hue: f64) -> Option<Number> {
    if !(HUE_MIN..=HUE_MAX).contains(&hue) {
        return None;
    }
    serde_json::from_str(&hue.to_string()).ok()
}

fn current_version() -> u32 {
    CONFIG_VERSION
}

impl Default for BoardConfig {
    fn default() -> BoardConfig {
        BoardConfig {
            version: CONFIG_VERSION,
            tags: None,
            sections: None,
            default_section: None,
            archive_section: None,
            auto_tags: None,
            repos: None,
            hue: None,
            layout: None,
            requests: None,
            extra: Map::new(),
        }
    }
}

impl BoardConfig {
    pub fn parse(text: &str) -> Result<BoardConfig, serde_json::Error> {
        if text::trim(text).is_empty() {
            return Ok(BoardConfig::default());
        }
        serde_json::from_str(text)
    }

    pub fn render(&self) -> Result<String, serde_json::Error> {
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        Ok(text)
    }

    #[must_use]
    pub fn section_entries(&self) -> &[SectionEntry] {
        self.sections.as_deref().unwrap_or_default()
    }

    pub fn add_section(
        &mut self,
        id: &SectionId,
        name: &str,
        parent: Option<&SectionId>,
        note: &str,
        hue: Option<f64>,
    ) -> Result<(), Invalid> {
        let name = text::single_spaced(name);
        if section_name(&name).is_empty() {
            return Err(Invalid::EmptySectionName);
        }
        let parent = parent.map(SectionId::as_str);
        let taken = self.section_entries().iter().any(|entry| {
            entry.parent.as_deref() == parent && same_section_name(&entry.name, &name)
        });
        if taken {
            return Err(Invalid::SectionNameTaken { name });
        }
        let hue = match hue {
            Some(hue) => Some(written_hue(hue).ok_or(Invalid::HueOutOfRange)?),
            None => None,
        };
        self.sections.get_or_insert_default().push(SectionEntry {
            id: id.as_str().to_owned(),
            name,
            parent: parent.map(str::to_owned),
            note: text::trim(note).to_owned(),
            hue,
            extra: Map::new(),
        });
        self.version = self.version.max(CONFIG_VERSION);
        Ok(())
    }
}
