use serde_json::json;
use wb_model::{
    BoardEvent, BoardId, BoardInfo, BoardSnapshot, BodyChange, Change, DraftSection, Note,
    NoteCard, NoteDraft, NoteEdit, NoteId, Op, Outcome, SectionId, SectionNode, SectionRef,
    StaleBase, Tag, diff, edit_note,
};

fn note_id(text: &str) -> NoteId {
    NoteId::parse(text).expect("note id")
}

fn card(id: &str, title: &str) -> NoteCard {
    NoteCard {
        id: note_id(id),
        title: title.to_owned(),
        body: String::new(),
        tags: Vec::new(),
        section: None,
    }
}

fn section(id: &str, name: &str) -> SectionNode {
    SectionNode {
        id: SectionId::parse(id).expect("section id"),
        name: name.to_owned(),
        hue: None,
        children: Vec::new(),
    }
}

fn snapshot(sections: Vec<SectionNode>, notes: Vec<NoteCard>) -> BoardSnapshot {
    BoardSnapshot {
        board: BoardInfo {
            id: BoardId::parse("garden").expect("board id"),
            name: "Garden".to_owned(),
        },
        sections,
        notes,
        strays: Vec::new(),
    }
}

fn plant_list() -> Note {
    Note {
        title: "Plant list".to_owned(),
        body: "Tomatoes".to_owned(),
        tags: Tag::list(&["vegetable"]),
        extra: vec![("variety".to_owned(), "San Marzano".to_owned())],
        ..Note::new(note_id("n1"))
    }
}

fn set(base: Option<&str>, to: &str) -> Change<String> {
    Change {
        base: base.map(str::to_owned),
        to: to.to_owned(),
    }
}

#[test]
fn two_equal_snapshots_differ_in_nothing() {
    let board = snapshot(vec![section("s1", "Beds")], vec![card("n1", "Plant list")]);
    assert_eq!(diff(&board, &board.clone()), Vec::new());
}

#[test]
fn the_difference_names_changed_added_and_removed_notes_and_new_sections() {
    let before = snapshot(
        vec![section("s1", "Beds")],
        vec![
            card("n1", "Plant list"),
            card("n2", "Seeds"),
            card("n3", "Tools"),
        ],
    );
    let after = snapshot(
        vec![section("s1", "Beds"), section("s2", "Harvest")],
        vec![
            card("n1", "Plant list 2027"),
            card("n3", "Tools"),
            card("n4", "Compost"),
        ],
    );
    assert_eq!(
        diff(&before, &after),
        [
            BoardEvent::SectionsChanged {
                sections: after.sections.clone()
            },
            BoardEvent::NoteChanged {
                note: card("n1", "Plant list 2027")
            },
            BoardEvent::NoteChanged {
                note: card("n4", "Compost")
            },
            BoardEvent::NoteRemoved {
                note: note_id("n2")
            },
        ]
    );
}

#[test]
fn an_edit_changes_only_the_fields_it_names() {
    let edit = NoteEdit {
        title: Some(set(Some("Plant list"), "Plant  list\n2027")),
        body: None,
    };
    let edited = edit_note(&plant_list(), &edit).expect("applied");
    assert_eq!(
        edited,
        Note {
            title: "Plant list 2027".to_owned(),
            ..plant_list()
        }
    );
}

#[test]
fn an_edit_whose_base_no_longer_matches_is_refused() {
    let stale_body = NoteEdit {
        title: None,
        body: Some(BodyChange::Set(set(Some("Potatoes"), "Beans"))),
    };
    assert_eq!(edit_note(&plant_list(), &stale_body), Err(StaleBase::Body));
    let stale_title = NoteEdit {
        title: Some(set(Some("Seed list"), "Plants")),
        body: Some(BodyChange::Set(set(Some("Tomatoes"), "Beans"))),
    };
    assert_eq!(
        edit_note(&plant_list(), &stale_title),
        Err(StaleBase::Title)
    );
}

#[test]
fn an_edit_without_a_base_is_applied_to_whatever_is_there() {
    let blind = NoteEdit {
        title: None,
        body: Some(BodyChange::Set(set(None, "Beans\n\n"))),
    };
    assert_eq!(
        edit_note(&plant_list(), &blind).expect("applied").body,
        "Beans"
    );
}

#[test]
fn appended_text_becomes_a_new_paragraph() {
    let append = |text: &str| NoteEdit {
        title: None,
        body: Some(BodyChange::Append {
            text: text.to_owned(),
        }),
    };
    let once = edit_note(&plant_list(), &append("Beans\n")).expect("applied");
    assert_eq!(once.body, "Tomatoes\n\nBeans");
    let empty = Note {
        body: String::new(),
        ..plant_list()
    };
    assert_eq!(
        edit_note(&empty, &append("Beans")).expect("applied").body,
        "Beans"
    );
}

#[test]
fn operations_and_outcomes_cross_the_bridge_as_tagged_objects() {
    let op = Op::EditNote {
        note: note_id("n1"),
        edit: NoteEdit {
            title: None,
            body: Some(BodyChange::Set(set(Some("old"), "new"))),
        },
    };
    let sent = json!({
        "type": "editNote",
        "note": "n1",
        "edit": { "title": null, "body": { "type": "set", "base": "old", "to": "new" } }
    });
    assert_eq!(serde_json::to_value(&op).expect("json"), sent);
    assert_eq!(serde_json::from_value::<Op>(sent).expect("op"), op);
    assert_eq!(
        serde_json::to_value(Outcome::Conflict {
            current: card("n1", "T")
        })
        .expect("json"),
        json!({
            "type": "conflict",
            "current": { "id": "n1", "title": "T", "body": "", "tags": [], "section": null }
        })
    );
}

#[test]
fn a_draft_says_where_its_note_goes_in_one_of_three_ways() {
    let draft = |section: DraftSection| NoteDraft {
        title: "Plant list".to_owned(),
        section,
        ..NoteDraft::default()
    };
    let sent = |section: serde_json::Value| json!({ "title": "Plant list", "body": "", "section": section, "tags": [], "source": "" });
    let named = DraftSection::Named {
        section: SectionRef::Id {
            id: SectionId::parse("s1").expect("section id"),
        },
    };
    for (section, written) in [
        (DraftSection::Default, json!({ "type": "default" })),
        (DraftSection::NoSection, json!({ "type": "noSection" })),
        (
            named,
            json!({ "type": "named", "section": { "type": "id", "id": "s1" } }),
        ),
    ] {
        let draft = draft(section);
        assert_eq!(
            serde_json::to_value(&draft).expect("json"),
            sent(written.clone())
        );
        assert_eq!(
            serde_json::from_value::<NoteDraft>(sent(written)).expect("draft"),
            draft
        );
    }
    assert_eq!(NoteDraft::default().section, DraftSection::Default);
}
