use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use serde::{Deserialize, Serialize};

use crate::config::BoardConfig;
use crate::id::SectionId;
use crate::section_path::{SectionPath, same_section_name};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SectionNode {
    pub id: SectionId,
    pub name: String,
    pub note: String,
    pub hue: Option<f64>,
    pub children: Vec<SectionNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum SectionRef {
    Id { id: SectionId },
    Path { path: SectionPath },
}

impl fmt::Display for SectionRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SectionRef::Id { id } => write!(formatter, "with the id `{id}`"),
            SectionRef::Path { path } => write!(formatter, "at `{path}`"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Section {
    id: SectionId,
    name: String,
    note: String,
    parent: Option<SectionId>,
    hue: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SectionTree {
    sections: Vec<Section>,
}

impl SectionTree {
    #[must_use]
    pub fn from_config(config: &BoardConfig) -> SectionTree {
        let mut listed: Vec<Section> = Vec::new();
        for entry in config.section_entries() {
            let Ok(id) = SectionId::parse(&entry.id) else {
                continue;
            };
            if listed.iter().any(|section| section.id == id) {
                continue;
            }
            listed.push(Section {
                id,
                name: entry.name.clone(),
                note: entry.note.clone(),
                parent: entry
                    .parent
                    .as_deref()
                    .and_then(|parent| SectionId::parse(parent).ok()),
                hue: entry.hue.as_ref().and_then(serde_json::Number::as_f64),
            });
        }
        let parents: Vec<Option<SectionId>> = listed
            .iter()
            .map(|section| parent_outside_loops(&listed, section))
            .collect();
        for (section, parent) in listed.iter_mut().zip(parents) {
            section.parent = parent;
        }
        SectionTree { sections: listed }
    }

    #[must_use]
    pub fn contains(&self, id: &SectionId) -> bool {
        self.get(id).is_some()
    }

    #[must_use]
    pub fn resolve(&self, path: &SectionPath) -> Option<&SectionId> {
        let mut at = None;
        for name in path.names() {
            at = Some(self.child_named(at, name)?);
        }
        at
    }

    #[must_use]
    pub fn find(&self, reference: &SectionRef) -> Option<&SectionId> {
        match reference {
            SectionRef::Id { id } => self.get(id).map(|section| &section.id),
            SectionRef::Path { path } => self.resolve(path),
        }
    }

    #[must_use]
    pub fn lookup(&self, text: &str) -> Option<&SectionId> {
        SectionId::parse(crate::text::trim(text))
            .ok()
            .and_then(|id| self.get(&id))
            .map(|section| &section.id)
            .or_else(|| self.resolve(&SectionPath::parse(text)?))
    }

    #[must_use]
    pub fn path_of(&self, id: &SectionId) -> Option<SectionPath> {
        let mut names = Vec::new();
        let mut at = self.get(id);
        while let Some(section) = at {
            names.insert(0, section.name.as_str());
            at = section.parent.as_ref().and_then(|parent| self.get(parent));
        }
        SectionPath::from_names(&names)
    }

    #[must_use]
    pub fn nodes(&self) -> Vec<SectionNode> {
        self.nodes_inside(None)
    }

    fn nodes_inside(&self, parent: Option<&SectionId>) -> Vec<SectionNode> {
        self.children(parent)
            .map(|section| SectionNode {
                id: section.id.clone(),
                name: section.name.clone(),
                note: section.note.clone(),
                hue: section.hue,
                children: self.nodes_inside(Some(&section.id)),
            })
            .collect()
    }

    fn get(&self, id: &SectionId) -> Option<&Section> {
        self.sections.iter().find(|section| &section.id == id)
    }

    fn children<'tree, 'parent>(
        &'tree self,
        parent: Option<&'parent SectionId>,
    ) -> impl Iterator<Item = &'tree Section> + use<'tree, 'parent> {
        self.sections
            .iter()
            .filter(move |section| section.parent.as_ref() == parent)
    }

    fn child_named(&self, parent: Option<&SectionId>, name: &str) -> Option<&SectionId> {
        self.children(parent)
            .find(|section| same_section_name(&section.name, name))
            .map(|section| &section.id)
    }
}

fn parent_outside_loops(listed: &[Section], section: &Section) -> Option<SectionId> {
    let listed_parent = |of: &Section| {
        of.parent
            .as_ref()
            .and_then(|parent| listed.iter().find(|other| &other.id == parent))
    };
    let parent = listed_parent(section)?;
    let mut above = Some(parent);
    for _ in 0..listed.len() {
        match above {
            Some(ancestor) if ancestor.id == section.id => return None,
            Some(ancestor) => above = listed_parent(ancestor),
            None => break,
        }
    }
    Some(parent.id.clone())
}
