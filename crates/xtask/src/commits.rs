use std::path::Path;

use crate::finding::{self, Finding};
use crate::process;

const SECTIONS: [&str; 6] = [
    "Added",
    "Changed",
    "Deprecated",
    "Removed",
    "Fixed",
    "Security",
];
const SUMMARY_LIMIT: usize = 72;
const RECORD_END: char = '\u{1e}';
const FIELD_END: char = '\u{1f}';

pub fn check(root: &Path) -> Result<(), String> {
    let log = process::output(root, &["git", "log", "--format=%h%x1f%P%x1f%B%x1e", "HEAD"])?;
    let mut findings = Vec::new();
    for record in log.split(RECORD_END) {
        let mut fields = record.trim_start_matches('\n').splitn(3, FIELD_END);
        let (Some(hash), Some(parents), Some(message)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        findings.extend(commit_findings(hash, parents, message));
    }
    finding::report(&findings)
}

fn commit_findings(hash: &str, parents: &str, message: &str) -> Vec<Finding> {
    let commit = format!("commit {hash}");
    let mut findings = Vec::new();
    match parents.split_whitespace().count() {
        0 => return findings,
        1 => {}
        _ => findings.push(Finding::whole(
            &commit,
            "is a merge commit; the history is linear, so rebase the branch instead",
        )),
    }
    findings.extend(
        message_problems(message)
            .into_iter()
            .map(|(line, problem)| Finding::new(&commit, line, problem)),
    );
    findings
}

fn type_tag(summary: &str) -> Option<&str> {
    let (prefix, _) = summary.split_once(':')?;
    let name = prefix.trim_end_matches('!');
    let name = match name.split_once('(') {
        Some((name, scope)) if scope.ends_with(')') => name,
        Some(_) => return None,
        None => name,
    };
    (!name.is_empty() && name.chars().all(|c| c.is_ascii_alphabetic())).then_some(prefix)
}

fn is_trailer(line: &str) -> bool {
    line.split_once(": ").is_some_and(|(key, value)| {
        !key.is_empty()
            && !value.trim().is_empty()
            && key.starts_with(|c: char| c.is_ascii_alphabetic())
            && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    })
}

fn holds_two_sentences(text: &str) -> bool {
    text.match_indices(". ").any(|(at, _)| {
        text[at + 2..]
            .chars()
            .next()
            .is_some_and(char::is_uppercase)
    })
}

fn summary_problems(summary: &str) -> Vec<String> {
    let mut problems = Vec::new();
    if summary.trim().is_empty() {
        problems.push("the summary line is empty".to_owned());
        return problems;
    }
    let length = summary.chars().count();
    if length > SUMMARY_LIMIT {
        problems.push(format!(
            "`{summary}`: the summary has {length} characters; keep it at most {SUMMARY_LIMIT}"
        ));
    }
    if summary.ends_with('.') {
        problems.push(format!(
            "`{summary}`: the summary ends with a period; drop it"
        ));
    }
    if let Some(tag) = type_tag(summary) {
        problems.push(format!(
            "`{summary}`: the summary starts with the type tag `{tag}:`; say what changes instead"
        ));
    } else if !summary.starts_with(char::is_uppercase) {
        problems.push(format!(
            "`{summary}`: the summary starts with a lowercase letter; capitalize it"
        ));
    }
    problems
}

fn message_problems(message: &str) -> Vec<(usize, String)> {
    let lines: Vec<&str> = message.trim_end().lines().collect();
    let mut problems: Vec<(usize, String)> = summary_problems(lines.first().unwrap_or(&""))
        .into_iter()
        .map(|problem| (1, problem))
        .collect();
    if lines.get(1).is_some_and(|line| !line.trim().is_empty()) {
        problems.push((2, "a blank line must follow the summary".to_owned()));
    }
    let mut section: Option<usize> = None;
    let mut open_heading: Option<(usize, &str)> = None;
    let mut index = 2;
    while index < lines.len() {
        let line = lines[index];
        let number = index + 1;
        index += 1;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix("++ ") {
            if let Some((heading_number, heading)) = open_heading.replace((number, line)) {
                problems.push(empty_section(heading_number, heading));
            }
            match SECTIONS.iter().position(|known| *known == name) {
                None => problems.push((
                    number,
                    format!(
                        "`{line}`: unknown section; use one of {}",
                        SECTIONS.join(", ")
                    ),
                )),
                Some(position) if section.is_some_and(|last| position <= last) => problems.push((
                    number,
                    format!(
                        "`{line}`: sections must appear once each, in the order {}",
                        SECTIONS.join(", ")
                    ),
                )),
                Some(position) => section = Some(position),
            }
            if lines.get(index).is_some_and(|next| !next.trim().is_empty()) {
                problems.push((number, format!("`{line}`: a blank line must follow it")));
            }
            continue;
        }
        if let Some(first) = line.strip_prefix("- ") {
            open_heading = None;
            if section.is_none() {
                problems.push((
                    number,
                    format!("`{line}`: a bullet must sit under a `++ <Section>` heading"),
                ));
            }
            index = bullet_problems(&lines, index, first, &mut problems);
            continue;
        }
        if is_trailer(line) {
            problems.push((
                number,
                format!("`{line}`: trailer lines are not allowed; remove it"),
            ));
        } else {
            problems.push((
                number,
                format!("`{line}`: the body holds only `++ <Section>` headings and `- ` bullets"),
            ));
        }
    }
    if let Some((heading_number, heading)) = open_heading {
        problems.push(empty_section(heading_number, heading));
    }
    problems.sort_by_key(|(number, _)| *number);
    problems
}

fn bullet_problems(
    lines: &[&str],
    mut index: usize,
    first: &str,
    problems: &mut Vec<(usize, String)>,
) -> usize {
    let number = index;
    let line = lines[index - 1];
    let mut text = first.to_owned();
    while let Some(next) = lines.get(index).and_then(|next| next.strip_prefix("  ")) {
        if is_trailer(next.trim()) {
            problems.push((
                index + 1,
                format!("`{next}`: trailer lines are not allowed, also inside a bullet; remove it"),
            ));
        }
        text.push(' ');
        text.push_str(next.trim());
        index += 1;
    }
    if !text.ends_with('.') {
        problems.push((number, format!("`{line}`: a bullet must end with a period")));
    }
    if holds_two_sentences(&text) {
        problems.push((
            number,
            format!("`{line}`: a bullet must be one sentence; split it"),
        ));
    }
    if lines.get(index).is_some_and(|next| next.starts_with("- ")) {
        problems.push((
            number + 1,
            format!(
                "`{}`: bullets must be separated by a blank line",
                lines[index]
            ),
        ));
    }
    index
}

fn empty_section(number: usize, heading: &str) -> (usize, String) {
    (
        number,
        format!(
            "`{heading}`: a section holds at least one bullet; drop the heading or add the bullet"
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::{commit_findings, message_problems};

    fn problems(message: &str) -> Vec<String> {
        message_problems(message)
            .into_iter()
            .map(|(line, problem)| format!("{line}: {problem}"))
            .collect()
    }

    const GOOD: &str = "Add the board list\n\n++ Added\n\n- Add a list of boards so that people can pick one.\n\n- Add tests for the empty list.\n\n++ Changed\n\n- Sort boards by name.\n";

    #[test]
    fn a_well_formed_message_passes() {
        assert_eq!(problems(GOOD), Vec::<String>::new());
        assert_eq!(problems("Add the board list\n"), Vec::<String>::new());
    }

    #[test]
    fn a_wrapped_bullet_counts_as_one_bullet() {
        let message = "Add the board list\n\n++ Added\n\n- Add a list of boards so that people\n  can pick one.\n";
        assert_eq!(problems(message), Vec::<String>::new());
    }

    #[test]
    fn a_type_prefix_is_reported() {
        assert_eq!(
            problems("feat: add the board list\n"),
            vec![
                "1: `feat: add the board list`: the summary starts with the type tag `feat:`; say what changes instead"
            ]
        );
        assert_eq!(problems("fix(cli)!: Add x\n").len(), 1);
    }

    #[test]
    fn summary_rules_are_reported() {
        let long = format!("Add {}", "x".repeat(70));
        assert_eq!(problems(&long).len(), 1);
        assert_eq!(problems("Add the board list.\n").len(), 1);
        assert_eq!(problems("add the board list\n").len(), 1);
    }

    #[test]
    fn a_trailer_is_reported() {
        let message = format!("{GOOD}\nReviewed-by: Someone Else\n");
        assert_eq!(
            problems(&message),
            vec!["13: `Reviewed-by: Someone Else`: trailer lines are not allowed; remove it"]
        );
    }

    #[test]
    fn a_bullet_without_a_period_is_reported() {
        let message = "Add the board list\n\n++ Added\n\n- Add a list of boards\n";
        assert_eq!(
            problems(message),
            vec!["5: `- Add a list of boards`: a bullet must end with a period"]
        );
    }

    #[test]
    fn section_rules_are_reported() {
        let message = "Add x\n\n++ Changed\n\n- Change x.\n\n++ Added\n\n- Add y.\n\n++ Notes\n- Add z.\n- Add w.\n";
        assert_eq!(
            problems(message),
            vec![
                "7: `++ Added`: sections must appear once each, in the order Added, Changed, Deprecated, Removed, Fixed, Security",
                "11: `++ Notes`: unknown section; use one of Added, Changed, Deprecated, Removed, Fixed, Security",
                "11: `++ Notes`: a blank line must follow it",
                "13: `- Add w.`: bullets must be separated by a blank line",
            ]
        );
    }

    #[test]
    fn two_sentences_in_one_bullet_are_reported() {
        let message = "Add x\n\n++ Added\n\n- Add x. Then add y.\n";
        assert_eq!(problems(message).len(), 1);
    }

    #[test]
    fn a_type_tag_in_any_letter_case_is_reported() {
        assert_eq!(problems("Feat: Add the board list\n").len(), 1);
        assert_eq!(problems("Fix(ui): Tidy the board list\n").len(), 1);
        assert_eq!(
            problems("Add the board list: sorted by name\n"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_merge_commit_is_reported_and_a_root_commit_is_skipped() {
        let merge: Vec<String> = commit_findings("abc1234", "1111111 2222222", GOOD)
            .into_iter()
            .map(|finding| finding.to_string())
            .collect();
        assert_eq!(
            merge,
            vec![
                "commit abc1234: is a merge commit; the history is linear, so rebase the branch instead"
            ]
        );
        assert!(commit_findings("abc1234", "", "anything at all").is_empty());
    }

    #[test]
    fn an_empty_section_is_reported() {
        assert_eq!(
            problems("Add x\n\n++ Added\n\n++ Changed\n\n- Change x.\n\n++ Fixed\n"),
            vec![
                "3: `++ Added`: a section holds at least one bullet; drop the heading or add the bullet",
                "9: `++ Fixed`: a section holds at least one bullet; drop the heading or add the bullet",
            ]
        );
    }

    #[test]
    fn a_trailer_inside_a_bullet_is_reported() {
        let message =
            "Add x\n\n++ Added\n\n- Add x so that people\n  Co-Authored-By: Someone <someone>\n";
        assert_eq!(
            problems(message),
            vec![
                "5: `- Add x so that people`: a bullet must end with a period",
                "6: `Co-Authored-By: Someone <someone>`: trailer lines are not allowed, also inside a bullet; remove it",
            ]
        );
    }

    #[test]
    fn free_text_in_the_body_is_reported() {
        let message = "Add x\n\nSome prose about it.\n";
        assert_eq!(problems(message).len(), 1);
    }
}
