use wb_model::{BoardId, IdError, NoteId, SectionId};

#[test]
fn note_and_section_ids_take_letters_digits_underscore_and_dash() {
    assert_eq!(
        NoteId::parse("n4k2x9").map(|id| id.to_string()),
        Ok("n4k2x9".to_owned())
    );
    assert!(SectionId::parse("Beds_2-west").is_ok());
    assert!(NoteId::parse(&"a".repeat(40)).is_ok());
}

#[test]
fn note_and_section_ids_reject_bad_input() {
    assert_eq!(NoteId::parse(""), Err(IdError::Empty));
    assert_eq!(
        NoteId::parse(&"a".repeat(41)),
        Err(IdError::TooLong {
            max: 40,
            actual: 41
        })
    );
    assert_eq!(
        SectionId::parse("plant list"),
        Err(IdError::InvalidCharacter(' '))
    );
    assert_eq!(
        SectionId::parse("garden/beds"),
        Err(IdError::InvalidCharacter('/'))
    );
    assert_eq!(NoteId::parse("blüte"), Err(IdError::InvalidCharacter('ü')));
}

#[test]
fn board_ids_are_folder_names_joined_by_slashes() {
    assert_eq!(
        BoardId::parse("garden").map(|id| id.to_string()),
        Ok("garden".to_owned())
    );
    assert!(BoardId::parse("kitchen/recipes").is_ok());
    assert!(BoardId::parse("2026-plans/q1").is_ok());
    assert!(BoardId::parse(&"a".repeat(64)).is_ok());
}

#[test]
fn board_ids_reject_bad_folder_names() {
    assert_eq!(BoardId::parse(""), Err(IdError::Empty));
    assert_eq!(BoardId::parse("garden//beds"), Err(IdError::EmptyFolder));
    assert_eq!(BoardId::parse("garden/"), Err(IdError::EmptyFolder));
    assert_eq!(
        BoardId::parse("Garden"),
        Err(IdError::InvalidFolder {
            folder: "Garden".to_owned()
        })
    );
    assert_eq!(
        BoardId::parse("-garden"),
        Err(IdError::InvalidFolder {
            folder: "-garden".to_owned()
        })
    );
    assert_eq!(
        BoardId::parse("kitchen/my_recipes"),
        Err(IdError::InvalidFolder {
            folder: "my_recipes".to_owned()
        })
    );
    assert_eq!(
        BoardId::parse(&"a".repeat(65)),
        Err(IdError::FolderTooLong {
            folder: "a".repeat(65),
            max: 64
        })
    );
}

#[test]
fn ids_cross_as_plain_strings_and_are_parsed_on_the_way_in() {
    let id = BoardId::parse("kitchen/recipes").expect("valid id");
    assert_eq!(
        serde_json::to_string(&id).expect("serialize"),
        "\"kitchen/recipes\""
    );
    let back: BoardId = serde_json::from_str("\"kitchen/recipes\"").expect("deserialize");
    assert_eq!(back, id);
    let refused = serde_json::from_str::<NoteId>("\"not an id\"");
    assert!(refused.is_err());
}
