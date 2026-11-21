use wb_core::NoteFile;
use wb_model::{Note, NoteKind, text};

const LABEL_LENGTH: usize = 60;

pub fn is_rules(file: &NoteFile) -> bool {
    file.note.kind == NoteKind::Rules
}

pub fn label(note: &Note) -> String {
    if !note.title.is_empty() {
        return note.title.clone();
    }
    let first = text::trim(
        note.body
            .lines()
            .find(|line| !text::trim(line).is_empty())
            .unwrap_or_default(),
    );
    if first.chars().count() <= LABEL_LENGTH {
        return first.to_owned();
    }
    let cut: String = first.chars().take(LABEL_LENGTH - 1).collect();
    format!("{}…", cut.trim_end())
}

pub fn tag_names(note: &Note) -> Vec<String> {
    note.tags.iter().map(ToString::to_string).collect()
}

pub fn section_of(note: &Note) -> Option<String> {
    note.placement.section.as_ref().map(ToString::to_string)
}
