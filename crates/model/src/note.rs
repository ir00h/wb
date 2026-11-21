use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::Write;

use crate::id::NoteId;
use crate::section_path::SectionPath;
use crate::tag::Tag;
use crate::text;

const FENCE: &str = "---";
const BYTE_ORDER_MARK: char = '\u{feff}';
const SLUG_MAX_BYTES: usize = 48;
const FALLBACK_SLUG: &str = "note";
const PLAIN_VALUE_PUNCTUATION: &str = " .,()/&+-_";
const PLAIN_TAG_PUNCTUATION: &str = "-/_.";

pub const KNOWN_KEYS: [&str; 8] = [
    "id", "kind", "section", "stack", "order", "tags", "source", "board",
];

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum NoteKind {
    #[default]
    Plain,
    Board,
    Image,
    Rules,
    Other(String),
}

impl NoteKind {
    fn read(written: &str) -> NoteKind {
        match Tag::new(&scalar(written)).as_ref().map(Tag::as_str) {
            Some("board") => NoteKind::Board,
            Some("image") => NoteKind::Image,
            Some("rules") => NoteKind::Rules,
            _ => NoteKind::Other(written.to_owned()),
        }
    }

    fn name(&self) -> Option<&'static str> {
        match self {
            NoteKind::Board => Some("board"),
            NoteKind::Image => Some("image"),
            NoteKind::Rules => Some("rules"),
            NoteKind::Plain | NoteKind::Other(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Placement {
    pub section: Option<SectionPath>,
    pub stack: Option<String>,
    pub order: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub id: NoteId,
    pub kind: NoteKind,
    pub placement: Placement,
    pub tags: Vec<Tag>,
    pub source: String,
    pub board: Option<String>,
    pub extra: Vec<(String, String)>,
    pub title: String,
    pub body: String,
}

impl Note {
    #[must_use]
    pub fn new(id: NoteId) -> Note {
        Note {
            id,
            kind: NoteKind::Plain,
            placement: Placement::default(),
            tags: Vec::new(),
            source: String::new(),
            board: None,
            extra: Vec::new(),
            title: String::new(),
            body: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Parsed {
    Note(Box<Note>),
    Stray { title: String, body: String },
}

#[must_use]
pub fn clean_title(title: &str) -> String {
    text::single_spaced(title)
}

#[must_use]
pub fn clean_body(body: &str) -> String {
    body.replace("\r\n", "\n")
        .trim_start_matches('\n')
        .trim_end_matches(text::is_space)
        .to_owned()
}

#[must_use]
pub fn parse(text: &str) -> Parsed {
    let text = text
        .trim_start_matches(BYTE_ORDER_MARK)
        .replace("\r\n", "\n");
    let (meta, rest) = split_front_matter(&text);
    let (title, body) = split_title(rest.trim_start_matches('\n'));
    let body = clean_body(body);

    let written = |key: &str| {
        meta.iter()
            .rev()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    };
    let Some(id) = written("id").and_then(|value| NoteId::parse(&scalar(value)).ok()) else {
        return Parsed::Stray { title, body };
    };
    let cleaned = |key: &str| {
        written(key)
            .map(|value| text::trim(&scalar(value)).to_owned())
            .filter(|value| !value.is_empty())
    };

    Parsed::Note(Box::new(Note {
        id,
        kind: written("kind").map_or(NoteKind::Plain, NoteKind::read),
        placement: Placement {
            section: written("section").and_then(|value| SectionPath::parse(&scalar(value))),
            stack: cleaned("stack"),
            order: cleaned("order")
                .and_then(|value| value.parse::<f64>().ok())
                .filter(|order| order.is_finite()),
        },
        tags: written("tags").map(read_tags).unwrap_or_default(),
        source: written("source").map(scalar).unwrap_or_default(),
        board: written("board")
            .map(scalar)
            .filter(|board| !board.is_empty()),
        extra: meta
            .iter()
            .filter(|(key, _)| !KNOWN_KEYS.contains(&key.as_str()))
            .cloned()
            .collect(),
        title,
        body,
    }))
}

#[must_use]
pub fn render(note: &Note) -> String {
    let mut out = format!("{FENCE}\nid: {}\n", note.id);
    match &note.kind {
        NoteKind::Other(written) if can_keep_value(written) => push_kept(&mut out, "kind", written),
        kind => {
            if let Some(name) = kind.name() {
                push_line(&mut out, "kind", name);
            }
        }
    }
    if let Some(section) = &note.placement.section {
        push_line(&mut out, "section", &plain_or_quoted(&section.to_string()));
    }
    if let Some(stack) = cleaned(note.placement.stack.as_deref()) {
        push_line(&mut out, "stack", &plain_or_quoted(stack));
    }
    if let Some(order) = note.placement.order.filter(|order| order.is_finite()) {
        push_line(&mut out, "order", &order.to_string());
    }
    let tags = Tag::list(&note.tags);
    if !tags.is_empty() {
        let items: Vec<String> = tags.iter().map(tag_item).collect();
        push_line(&mut out, "tags", &format!("[{}]", items.join(", ")));
    }
    if !note.source.is_empty() {
        push_line(&mut out, "source", &plain_or_quoted(&note.source));
    }
    if let Some(board) = cleaned(note.board.as_deref()) {
        push_line(&mut out, "board", board);
    }
    for (key, value) in &note.extra {
        if can_keep(key, value) {
            push_kept(&mut out, key, value);
        }
    }
    out.push_str(FENCE);
    out.push_str("\n\n");

    let title = clean_title(&note.title);
    let body = clean_body(&note.body);
    if !title.is_empty() || starts_with_heading(&body) {
        let _ = writeln!(out, "# {title}");
        if !body.is_empty() {
            out.push('\n');
        }
    }
    if !body.is_empty() {
        out.push_str(&body);
        out.push('\n');
    }
    out
}

#[must_use]
pub fn file_name(note: &Note) -> String {
    if note.kind == NoteKind::Rules {
        return format!("rules-{}.md", note.id);
    }
    let first_line = note
        .body
        .lines()
        .map(without_pictures)
        .find(|line| !text::trim(line).is_empty())
        .unwrap_or_default();
    let kind = note.kind.name().unwrap_or_default();
    let tag = note.tags.first().map(Tag::as_str).unwrap_or_default();
    let base = [note.title.as_str(), first_line.as_str(), kind, tag]
        .into_iter()
        .map(slug)
        .find(|candidate| !candidate.is_empty())
        .unwrap_or_else(|| FALLBACK_SLUG.to_owned());
    format!("{base}-{}.md", note.id)
}

#[must_use]
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    for character in text.chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
        if out.len() >= SLUG_MAX_BYTES {
            break;
        }
    }
    out.trim_end_matches('-').to_owned()
}

fn push_line(out: &mut String, key: &str, value: &str) {
    let _ = writeln!(out, "{key}: {value}");
}

fn push_kept(out: &mut String, key: &str, value: &str) {
    let gap = if value.is_empty() || value.starts_with('\n') {
        ""
    } else {
        " "
    };
    let _ = writeln!(out, "{key}:{gap}{value}");
}

fn cleaned(value: Option<&str>) -> Option<&str> {
    value.map(text::trim).filter(|value| !value.is_empty())
}

fn without_pictures(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(start) = rest.find("![") {
        let picture = &rest[start..];
        let end = picture
            .find("](")
            .and_then(|middle| picture[middle..].find(')').map(|close| middle + close + 1));
        let Some(end) = end else { break };
        out.push_str(&rest[..start]);
        rest = &picture[end..];
    }
    out + rest
}

fn is_heading(line: &str) -> bool {
    line == "#" || line.starts_with("# ") || line.starts_with("#\t")
}

fn starts_with_heading(body: &str) -> bool {
    body.lines().next().is_some_and(is_heading)
}

fn heading_text(line: &str) -> String {
    text::trim(line.trim_start_matches('#')).to_owned()
}

fn split_title(rest: &str) -> (String, &str) {
    match rest.split_once('\n') {
        Some((first, after)) if is_heading(first) => (heading_text(first), after),
        None if is_heading(rest) => (heading_text(rest), ""),
        _ => (String::new(), rest),
    }
}

fn split_front_matter(text: &str) -> (Vec<(String, String)>, &str) {
    let mut meta: Vec<(String, String)> = Vec::new();
    let Some(after_open) = text.strip_prefix("---\n") else {
        return (meta, text);
    };
    let Some(end) = after_open.find("\n---") else {
        return (meta, text);
    };
    let rest = &after_open[end + "\n---".len()..];
    if !(rest.is_empty() || rest.starts_with('\n')) {
        return (meta, text);
    }
    for line in after_open[..end].lines() {
        if let Some((key, value)) = key_line(line) {
            meta.push((key.to_owned(), text::trim(value).to_owned()));
        } else if let Some((_, value)) = meta.last_mut() {
            value.push('\n');
            value.push_str(line);
        }
    }
    (meta, rest)
}

fn key_line(line: &str) -> Option<(&str, &str)> {
    if line.starts_with([' ', '\t', '-', '#']) {
        return None;
    }
    let (key, value) = line.split_once(':')?;
    let key = text::trim(key);
    (!key.is_empty()).then_some((key, value))
}

fn can_keep(key: &str, value: &str) -> bool {
    key_line(&format!("{key}:")).is_some_and(|(read_back, _)| read_back == key)
        && !KNOWN_KEYS.contains(&key)
        && can_keep_value(value)
}

fn can_keep_value(value: &str) -> bool {
    value
        .lines()
        .skip(1)
        .all(|line| key_line(line).is_none() && !line.starts_with(FENCE))
}

fn read_tags(written: &str) -> Vec<Tag> {
    let (inline, below) = written.split_once('\n').unwrap_or((written, ""));
    let inline = text::trim(inline);
    let inline = inline
        .strip_prefix('[')
        .map_or(inline, |list| list.strip_suffix(']').unwrap_or(list));
    let mut items = split_list(inline);
    items.extend(
        below
            .lines()
            .filter_map(|line| text::trim(line).strip_prefix('-'))
            .map(|item| text::trim(item).to_owned()),
    );
    let items: Vec<String> = items.iter().map(|item| unquoted(item)).collect();
    Tag::list(&items)
}

fn split_list(text: &str) -> Vec<String> {
    let (mut items, mut item) = (Vec::new(), String::new());
    let (mut quote, mut escaped) = (None, false);
    for character in text.chars() {
        match quote {
            Some(open) => {
                if escaped {
                    escaped = false;
                } else if character == '\\' && open == '"' {
                    escaped = true;
                } else if character == open {
                    quote = None;
                }
                item.push(character);
            }
            None if character == ',' => {
                items.push(text::trim(&core::mem::take(&mut item)).to_owned());
            }
            None => {
                if (character == '"' || character == '\'') && text::trim(&item).is_empty() {
                    quote = Some(character);
                }
                item.push(character);
            }
        }
    }
    items.push(text::trim(&item).to_owned());
    items
}

fn tag_item(tag: &Tag) -> String {
    let plain = tag
        .as_str()
        .chars()
        .all(|character| character.is_alphanumeric() || PLAIN_TAG_PUNCTUATION.contains(character));
    if plain {
        tag.as_str().to_owned()
    } else {
        quoted(tag.as_str())
    }
}

fn plain_or_quoted(value: &str) -> String {
    let plain = !value.is_empty()
        && text::trim(value) == value
        && value.chars().all(|character| {
            character.is_alphanumeric() || PLAIN_VALUE_PUNCTUATION.contains(character)
        });
    if plain {
        value.to_owned()
    } else {
        quoted(value)
    }
}

fn quoted(value: &str) -> String {
    serde_json::Value::from(value).to_string()
}

fn scalar(written: &str) -> String {
    unquoted(text::trim(written.lines().next().unwrap_or_default()))
}

fn unquoted(value: &str) -> String {
    if value.starts_with('"')
        && let Ok(text) = serde_json::from_str::<String>(value)
    {
        return text;
    }
    if value.len() > 1 && value.starts_with('\'') && value.ends_with('\'') {
        return value[1..value.len() - 1].to_owned();
    }
    value.to_owned()
}
