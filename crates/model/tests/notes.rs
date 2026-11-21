use wb_model::{
    Note, NoteId, NoteKind, Parsed, Placement, SectionPath, Tag, file_name, parse, render, slug,
};

fn note(title: &str, body: &str) -> Note {
    Note {
        title: title.to_owned(),
        body: body.to_owned(),
        ..Note::new(NoteId::parse("n3").expect("id"))
    }
}

fn read(text: &str) -> Note {
    match parse(text) {
        Parsed::Note(note) => *note,
        Parsed::Stray { .. } => panic!("no id in {text:?}"),
    }
}

fn round_trip(note: &Note) -> Note {
    read(&render(note))
}

fn tags(names: &[&str]) -> Vec<Tag> {
    Tag::list(names)
}

fn tag_names(note: &Note) -> Vec<&str> {
    note.tags.iter().map(Tag::as_str).collect()
}

fn in_section(path: &str) -> Placement {
    Placement {
        section: SectionPath::parse(path),
        ..Placement::default()
    }
}

#[test]
fn renders_the_documented_format() {
    let written = Note {
        placement: in_section("Beds / Raised beds"),
        tags: tags(&["herb", "season/spring", "open-question"]),
        source: "seed catalogue, March".to_owned(),
        extra: vec![("variety".to_owned(), "San Marzano".to_owned())],
        ..note("Basil row", "Sow between the tomatoes.")
    };
    assert_eq!(
        render(&written),
        "---\nid: n3\nsection: Beds / Raised beds\ntags: [herb, season/spring, open-question]\nsource: seed catalogue, March\nvariety: San Marzano\n---\n\n# Basil row\n\nSow between the tomatoes.\n"
    );
}

#[test]
fn stack_and_order_are_written_after_the_section() {
    let written = Note {
        placement: Placement {
            section: SectionPath::parse("Harvest"),
            stack: Some("Picking round".to_owned()),
            order: Some(1536.0),
        },
        tags: tags(&["chore"]),
        ..note("T", "")
    };
    let text = render(&written);
    assert_eq!(
        text,
        "---\nid: n3\nsection: Harvest\nstack: Picking round\norder: 1536\ntags: [chore]\n---\n\n# T\n"
    );
    assert_eq!(read(&text).placement, written.placement);

    let with_order = |order: f64| Note {
        placement: Placement {
            order: Some(order),
            ..Placement::default()
        },
        ..note("", "x")
    };
    assert_eq!(round_trip(&with_order(0.375)).placement.order, Some(0.375));
    assert_eq!(round_trip(&with_order(-2.0)).placement.order, Some(-2.0));
    assert!(!render(&with_order(f64::NAN)).contains("order"));
    assert_eq!(
        read("---\nid: a\norder: first\n---\n").placement.order,
        None
    );
    assert!(read("---\nid: a\norder: 3\n---\n").extra.is_empty());
}

#[test]
fn only_special_cards_write_a_kind() {
    assert!(!render(&note("Plain", "")).contains("kind:"));
    let board = Note {
        kind: NoteKind::Board,
        board: Some("seeds".to_owned()),
        ..note("Seeds", "")
    };
    assert_eq!(
        render(&board),
        "---\nid: n3\nkind: board\nboard: seeds\n---\n\n# Seeds\n"
    );
    assert_eq!(round_trip(&board).board.as_deref(), Some("seeds"));
    let image = Note {
        kind: NoteKind::Image,
        ..note("", "![](assets/x.png)")
    };
    assert_eq!(round_trip(&image).kind, NoteKind::Image);
    assert_eq!(read("---\nid: a\nkind: Board\n---\n").kind, NoteKind::Board);
}

#[test]
fn an_unrecognised_kind_is_kept_as_written_and_is_not_a_tag() {
    let text = "---\nid: n3\nkind: Idea\ntags: [herb]\n---\n\n# Marigolds\n";
    let read = read(text);
    assert_eq!(read.kind, NoteKind::Other("Idea".to_owned()));
    assert_eq!(tag_names(&read), ["herb"]);
    assert_eq!(render(&read), text);
}

#[test]
fn title_and_body_survive_a_round_trip() {
    let read = round_trip(&note("A title", "First line\n\n- a list\n- of things"));
    assert_eq!(read.title, "A title");
    assert_eq!(read.body, "First line\n\n- a list\n- of things");
    assert_eq!(read.id.as_str(), "n3");
    assert_eq!(read.kind, NoteKind::Plain);
}

#[test]
fn section_tags_and_source_survive_a_round_trip() {
    let written = Note {
        placement: in_section("Outer / Quotes: \"here\""),
        tags: tags(&["question", "bed/north-1"]),
        source: "Garden diary".to_owned(),
        ..note("T", "b")
    };
    let read = round_trip(&written);
    assert_eq!(read.placement.section, written.placement.section);
    assert_eq!(tag_names(&read), ["question", "bed/north-1"]);
    assert_eq!(read.source, "Garden diary");
}

#[test]
fn unknown_keys_are_kept_exactly_and_in_order() {
    let text = "---\nid: n3\nsection: Beds\ntags: [herb]\nsource: me\nvariety: Genovese\naliases:\n  - sweet basil\n  - great basil\nsown: \"2026-04-17\"\nempty:\nvariety: Thai\n---\n\n# T\n\nbody\n";
    let read = read(text);
    assert_eq!(
        read.extra,
        [
            ("variety".to_owned(), "Genovese".to_owned()),
            (
                "aliases".to_owned(),
                "\n  - sweet basil\n  - great basil".to_owned()
            ),
            ("sown".to_owned(), "\"2026-04-17\"".to_owned()),
            ("empty".to_owned(), String::new()),
            ("variety".to_owned(), "Thai".to_owned()),
        ]
    );
    assert_eq!(render(&read), text);
}

#[test]
fn known_keys_come_first_and_unknown_ones_keep_their_order() {
    let read = read("---\nzeta: 1\ntags: a\nid: n3\nalpha: 2\n---\n\nbody");
    assert_eq!(
        render(&read),
        "---\nid: n3\ntags: [a]\nzeta: 1\nalpha: 2\n---\n\nbody\n"
    );
}

#[test]
fn a_kept_key_cannot_break_the_front_matter() {
    let written = Note {
        extra: vec![
            ("id".to_owned(), "x9".to_owned()),
            ("bad key\n".to_owned(), "v".to_owned()),
            ("ok".to_owned(), "a\n---\nb".to_owned()),
            ("fine".to_owned(), "yes".to_owned()),
        ],
        ..note("T", "")
    };
    assert_eq!(render(&written), "---\nid: n3\nfine: yes\n---\n\n# T\n");
}

#[test]
fn tags_are_read_in_every_usual_spelling() {
    let read_tags = |line: &str| {
        let read = read(&format!("---\nid: a\n{line}\n---\n"));
        tag_names(&read)
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        read_tags("tags: [herb, Bed/NORTH-1, shade]"),
        ["herb", "bed/north-1", "shade"]
    );
    assert_eq!(read_tags("tags: herb, shade"), ["herb", "shade"]);
    assert_eq!(
        read_tags("tags: [\"herb\", 'Open Question', \"x, y\", z]"),
        ["herb", "open-question", "x,-y", "z"]
    );
    assert_eq!(read_tags("tags:\n  - herb\n  - shade"), ["herb", "shade"]);
    assert_eq!(
        read_tags("tags: [ Herb , , herb, #shade ]"),
        ["herb", "shade"]
    );
    assert_eq!(read_tags("tags: []"), Vec::<String>::new());
    assert_eq!(read_tags("tags: solo"), ["solo"]);
}

#[test]
fn written_tags_are_unique_and_quoted_when_needed() {
    let written = Note {
        tags: tags(&["Herb", "herb", " ", "a,b"]),
        ..note("", "x")
    };
    let text = render(&written);
    assert!(text.contains("tags: [herb, \"a,b\"]\n"), "{text}");
    assert_eq!(tag_names(&read(&text)), ["herb", "a,b"]);
}

#[test]
fn only_ascii_spaces_tabs_and_line_ends_count_as_whitespace() {
    assert_eq!(
        Tag::new(" #Open \t Question ").map(String::from),
        Some("open-question".to_owned())
    );
    assert_eq!(
        Tag::new("next\u{85}line").map(String::from),
        Some("next\u{85}line".to_owned())
    );
    assert_eq!(Tag::new(" # "), None);
    let path = SectionPath::parse("Beds\u{a0}/\u{a0}Raised").expect("path");
    assert_eq!(path.names(), ["Beds\u{a0}/\u{a0}Raised"]);
}

#[test]
fn a_section_path_is_read_back_trimmed() {
    let section = |text: &str| read(text).placement.section.map(String::from);
    assert_eq!(
        section("---\nid: a\nsection:   Beds / Raised beds  \n---\n").as_deref(),
        Some("Beds / Raised beds")
    );
    assert_eq!(section("---\nid: a\nsection:\n---\n"), None);
    assert_eq!(section("---\nid: a\nsection: \"\"\n---\n"), None);
}

#[test]
fn section_paths_split_only_on_a_spaced_slash() {
    let names = |text: &str| SectionPath::parse(text).map(|path| path.names().to_vec());
    assert_eq!(
        names("Beds / Raised beds"),
        Some(vec!["Beds".to_owned(), "Raised beds".to_owned()])
    );
    assert_eq!(
        names("  Q1/Q2  /  Done "),
        Some(vec!["Q1/Q2".to_owned(), "Done".to_owned()])
    );
    assert_eq!(
        names(" / A /  / B / "),
        Some(vec!["A".to_owned(), "B".to_owned()])
    );
    assert_eq!(names(" / "), None);
    let written = SectionPath::from_names(&["Beds", "Q1 / Q2", " Raised   beds "]).expect("path");
    assert_eq!(written.to_string(), "Beds / Q1/Q2 / Raised beds");
    assert_eq!(SectionPath::parse(&written.to_string()), Some(written));
}

#[test]
fn a_note_without_a_title_keeps_its_body() {
    let read = round_trip(&note("", "just a sticky note"));
    assert_eq!(
        (read.title.as_str(), read.body.as_str()),
        ("", "just a sticky note")
    );
}

#[test]
fn a_body_starting_with_a_heading_is_not_mistaken_for_the_title() {
    let read = round_trip(&note("", "# Looks like a title\nbut is body"));
    assert_eq!(read.title, "");
    assert_eq!(read.body, "# Looks like a title\nbut is body");
    let read = round_trip(&note("Real title", "# Body heading\ntext"));
    assert_eq!(read.title, "Real title");
    assert_eq!(read.body, "# Body heading\ntext");
}

#[test]
fn awkward_values_are_quoted_and_read_back() {
    let written = Note {
        source: "x: y # \"z\"".to_owned(),
        placement: in_section("Quotes: \"here\""),
        ..note("Ünïcode & \"quotes\": ok?", "body")
    };
    let text = render(&written);
    assert!(text.contains("source: \"x: y # \\\"z\\\"\""));
    let read = read(&text);
    assert_eq!(read.source, "x: y # \"z\"");
    assert_eq!(read.placement.section, written.placement.section);
    assert_eq!(read.title, "Ünïcode & \"quotes\": ok?");
}

#[test]
fn a_title_is_written_on_one_line() {
    let text = render(&note("Plant\n  list\t2027", "body"));
    assert_eq!(text, "---\nid: n3\n---\n\n# Plant list 2027\n\nbody\n");
}

#[test]
fn a_file_without_front_matter_is_a_stray() {
    assert_eq!(
        parse("# Dropped in by hand\n\nA file with no front matter.\n"),
        Parsed::Stray {
            title: "Dropped in by hand".to_owned(),
            body: "A file with no front matter.".to_owned(),
        }
    );
}

#[test]
fn a_file_with_an_unusable_id_is_a_stray() {
    assert!(matches!(
        parse("---\nid: ../../etc\n---\n\ntext"),
        Parsed::Stray { .. }
    ));
}

#[test]
fn windows_line_endings_and_a_byte_order_mark_are_tolerated() {
    let read = read("\u{feff}---\r\nid: a1\r\n---\r\n\r\n# T\r\n\r\nline one\r\nline two\r\n");
    assert_eq!(read.id.as_str(), "a1");
    assert_eq!(read.title, "T");
    assert_eq!(read.body, "line one\nline two");
}

#[test]
fn file_names_are_readable_and_carry_the_id() {
    assert_eq!(
        file_name(&note("Plant list for 2027", "")),
        "plant-list-for-2027-n3.md"
    );
    assert_eq!(
        file_name(&note("", "First line of a sticky\nmore")),
        "first-line-of-a-sticky-n3.md"
    );
    assert_eq!(file_name(&note("", "")), "note-n3.md");
    let tagged = |title: &str| Note {
        tags: tags(&["idea"]),
        ..note(title, "")
    };
    assert_eq!(file_name(&tagged("")), "idea-n3.md");
    assert_eq!(file_name(&tagged("¿¡!?")), "idea-n3.md");
    assert_eq!(
        file_name(&note("", "![](assets/x.png)\nCaption below")),
        "caption-below-n3.md"
    );
    let picture = Note {
        kind: NoteKind::Image,
        ..tagged("")
    };
    assert_eq!(
        file_name(&Note {
            body: "![a bed](assets/x.png)".to_owned(),
            ..picture
        }),
        "image-n3.md"
    );
    let rules = Note {
        kind: NoteKind::Rules,
        ..note("How we work on this board", "- Plans go to Beds first.")
    };
    assert_eq!(file_name(&rules), "rules-n3.md");
}

#[test]
fn slugs_are_short_ascii_and_dash_separated() {
    assert_eq!(slug("  What's \"this\"?  "), "what-s-this");
    assert!(slug(&"word ".repeat(40)).len() <= 48);
}

#[test]
fn a_tag_is_well_formed_when_each_part_is_lowercase_letters_digits_and_three_marks() {
    for written in [
        "plan",
        "area/kitchen",
        "v1.2",
        "open-question",
        "to_do",
        "größe",
        "a/b/c",
    ] {
        let tag = Tag::new(written).expect("tag");
        assert!(tag.is_well_formed(), "`{tag}` was refused");
    }
    for written in ["plan!", "a//b", "/a", "a/", "what?", "50%", "a,b", "(x)"] {
        let tag = Tag::new(written).expect("tag");
        assert!(!tag.is_well_formed(), "`{tag}` was accepted");
    }
    assert!(Tag::new("Open Question").expect("tag").is_well_formed());
}
