use serde_json::{Value, json};
use wb_model::{BoardConfig, BoardFile, SectionId};

const WRITTEN: &str = r#"{
  "version": 1,
  "tags": [
    {
      "name": "herb",
      "hue": 200,
      "icon": "leaf"
    },
    {
      "name": "chore"
    }
  ],
  "sections": [
    {
      "id": "s1",
      "name": "Beds",
      "note": "What grows where",
      "hue": 120,
      "pinned": true
    },
    {
      "id": "s2",
      "name": "Harvest",
      "hue": 40.5,
      "layout": {
        "width": {
          "size": "fit",
          "max": 900
        },
        "view": "compact"
      },
      "next": "s1"
    }
  ],
  "defaultSection": "s1",
  "archiveSection": "s2",
  "autoTags": [
    "season"
  ],
  "hue": 210,
  "layout": {
    "arrange": "row"
  },
  "requests": [
    {
      "id": "r1",
      "kind": "tidy",
      "at": 1795274002749
    }
  ],
  "theme": {
    "paper": "kraft",
    "ink": "blue"
  }
}
"#;

#[test]
fn a_config_is_written_back_exactly_as_it_was_read() {
    let config = BoardConfig::parse(WRITTEN).expect("config");
    assert_eq!(config.render().expect("text"), WRITTEN);
}

#[test]
fn a_missing_or_blank_config_is_an_empty_one() {
    assert_eq!(
        BoardConfig::parse("  \n").expect("blank"),
        BoardConfig::default()
    );
    assert_eq!(
        BoardConfig::parse("{}").expect("empty"),
        BoardConfig::default()
    );
    assert!(BoardConfig::parse("{ \"sections\": [").is_err());
}

#[test]
fn adding_a_section_keeps_every_key_the_config_already_had() {
    let mut config = BoardConfig::parse(WRITTEN).expect("config");
    let id = SectionId::parse("s3").expect("id");
    config
        .add_section(&id, "Compost", None, "", None)
        .expect("added");
    let mut expected: Value = serde_json::from_str(WRITTEN).expect("json");
    expected["sections"]
        .as_array_mut()
        .expect("sections")
        .push(json!({ "id": "s3", "name": "Compost" }));
    assert_eq!(serde_json::to_value(&config).expect("json"), expected);
}

#[test]
fn a_later_config_version_is_not_lowered() {
    let mut config = BoardConfig::parse(r#"{ "version": 7 }"#).expect("config");
    let id = SectionId::parse("s1").expect("id");
    config
        .add_section(&id, "Beds", None, "", None)
        .expect("added");
    assert_eq!(config.version, 7);
}

#[test]
fn a_board_file_keeps_every_key_it_does_not_name() {
    let written = "{\n  \"version\": 2,\n  \"name\": \"Garden\",\n  \"cards\": {\n    \"n1\": {\n      \"x\": 60.0,\n      \"y\": 100.0\n    }\n  },\n  \"sections\": {\n    \"s1\": {\n      \"name\": \"Beds\"\n    }\n  },\n  \"zoom\": 1.5\n}\n";
    let file = BoardFile::parse(written).expect("board file");
    assert_eq!((file.version, file.name.as_str()), (2, "Garden"));
    assert_eq!(file.card_ids().collect::<Vec<_>>(), ["n1"]);
    assert_eq!(file.section_ids().collect::<Vec<_>>(), ["s1"]);
    assert_eq!(file.render().expect("text"), written);
}

#[test]
fn a_new_board_file_holds_the_format_version_the_name_and_no_cards_or_sections() {
    assert_eq!(
        BoardFile::new("Garden").render().expect("text"),
        "{\n  \"version\": 2,\n  \"name\": \"Garden\",\n  \"cards\": {},\n  \"sections\": {}\n}\n"
    );
}

#[test]
fn tag_colours_are_the_tags_with_a_hue_once_each() {
    let config = BoardConfig::parse(
        r#"{ "tags": [ { "name": "Vegetable", "hue": 120 }, { "name": "chore" }, { "name": "vegetable", "hue": 10 }, { "name": "herb", "hue": 200.5 } ] }"#,
    )
    .expect("config");
    let hues: Vec<(String, f64)> = config
        .tag_hues()
        .into_iter()
        .map(|entry| (entry.tag.to_string(), entry.hue))
        .collect();
    assert_eq!(
        hues,
        [("vegetable".to_owned(), 120.0), ("herb".to_owned(), 200.5)]
    );
}
