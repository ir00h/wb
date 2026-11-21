use serde::Serialize;

use crate::error::{Failure, Result};

const NOTHING: &str = "-";

pub fn emit<T: Serialize + ?Sized>(
    json: bool,
    value: &T,
    render: impl FnOnce(&T) -> String,
) -> Result<()> {
    let text = if json {
        serde_json::to_string_pretty(value)
            .map_err(|error| Failure(format!("could not write JSON: {error}")))?
    } else {
        render(value)
    };
    if !text.is_empty() {
        println!("{}", text.trim_end());
    }
    Ok(())
}

pub fn table(header: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = header.iter().map(|cell| cell.chars().count()).collect();
    for row in rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    let line = |cells: &[&str]| {
        let last = cells.len() - 1;
        let padded: Vec<String> = cells
            .iter()
            .zip(&widths)
            .enumerate()
            .map(|(index, (cell, width))| {
                if index == last {
                    (*cell).to_owned()
                } else {
                    format!("{cell:<width$}")
                }
            })
            .collect();
        padded.join("  ").trim_end().to_owned()
    };
    let mut out = vec![line(header)];
    out.extend(
        rows.iter()
            .map(|row| line(&row.iter().map(String::as_str).collect::<Vec<_>>())),
    );
    out.join("\n")
}

pub fn list_or_dash(items: &[String]) -> String {
    if items.is_empty() {
        NOTHING.to_owned()
    } else {
        items.join(", ")
    }
}

pub fn or_dash(value: Option<&str>) -> String {
    value.unwrap_or(NOTHING).to_owned()
}

pub fn count(number: usize, one: &str, many: &str) -> String {
    format!("{number} {}", if number == 1 { one } else { many })
}
