use serde_json::json;
use wb_model::{BoardConfig, Invalid, SectionId, SectionPath, SectionRef, SectionTree};

fn id(text: &str) -> SectionId {
    SectionId::parse(text).expect("section id")
}

fn config(sections: &serde_json::Value) -> BoardConfig {
    serde_json::from_value(json!({ "sections": sections })).expect("config")
}

fn sample() -> SectionTree {
    SectionTree::from_config(&config(&json!([
        { "id": "s1", "name": "Beds", "hue": 120 },
        { "id": "s2", "name": "Raised beds", "parent": "s1" },
        { "id": "s3", "name": "North / South", "parent": "s2" },
        { "id": "s4", "name": "Harvest" }
    ])))
}

fn resolved(tree: &SectionTree, path: &str) -> Option<String> {
    tree.resolve(&SectionPath::parse(path)?)
        .map(ToString::to_string)
}

#[test]
fn paths_are_built_from_the_outside_in() {
    let tree = sample();
    assert_eq!(
        tree.path_of(&id("s3")).map(String::from).as_deref(),
        Some("Beds / Raised beds / North/South")
    );
    assert_eq!(tree.path_of(&id("s9")), None);
}

#[test]
fn sections_are_found_by_path_without_regard_to_case_or_spacing() {
    let tree = sample();
    assert_eq!(
        resolved(&tree, "beds /  RAISED BEDS").as_deref(),
        Some("s2")
    );
    assert_eq!(
        resolved(&tree, "Beds / Raised beds / north/south").as_deref(),
        Some("s3")
    );
    assert_eq!(resolved(&tree, "Raised beds"), None);
    assert_eq!(tree.lookup("s4"), Some(&id("s4")));
    assert_eq!(tree.lookup("Beds / Raised beds"), Some(&id("s2")));
    assert_eq!(tree.lookup(""), None);
    assert_eq!(tree.find(&SectionRef::Id { id: id("s9") }), None);
}

#[test]
fn the_tree_nests_children_in_listed_order() {
    let nodes = sample().nodes();
    assert_eq!(nodes.len(), 2);
    assert_eq!(
        (nodes[0].name.as_str(), nodes[0].hue),
        ("Beds", Some(120.0))
    );
    assert_eq!(nodes[0].children[0].children[0].name, "North / South");
    assert_eq!(nodes[1].id, id("s4"));
}

#[test]
fn loops_and_missing_parents_make_top_level_sections() {
    let tree = SectionTree::from_config(&config(&json!([
        { "id": "x", "name": "X", "parent": "y" },
        { "id": "y", "name": "Y", "parent": "x" },
        { "id": "z", "name": "Z", "parent": "gone" },
        { "id": "w", "name": "W", "parent": "w" },
        { "id": "not an id", "name": "Skipped" }
    ])));
    let top: Vec<String> = tree.nodes().into_iter().map(|node| node.name).collect();
    assert_eq!(top, ["X", "Y", "Z", "W"]);
}

#[test]
fn the_first_listed_of_two_sections_with_one_name_is_found() {
    let tree = SectionTree::from_config(&config(&json!([
        { "id": "a", "name": "Beds" },
        { "id": "b", "name": "beds" }
    ])));
    assert_eq!(resolved(&tree, "BEDS").as_deref(), Some("a"));
}

#[test]
fn a_section_is_added_at_the_end_under_its_parent() {
    let mut config = config(&json!([{ "id": "s1", "name": "Beds" }]));
    config
        .add_section(&id("s2"), "  Raised   beds ", Some(&id("s1")))
        .expect("added");
    assert_eq!(
        serde_json::to_value(&config).expect("json")["sections"][1],
        json!({ "id": "s2", "name": "Raised beds", "parent": "s1" })
    );
}

#[test]
fn a_second_section_with_the_same_name_in_the_same_place_is_refused() {
    let mut config = config(&json!([{ "id": "s1", "name": "Beds" }]));
    assert_eq!(
        config.add_section(&id("s2"), "beds", None),
        Err(Invalid::SectionNameTaken {
            name: "beds".to_owned()
        })
    );
    assert_eq!(
        config.add_section(&id("s2"), " \t", None),
        Err(Invalid::EmptySectionName)
    );
    assert!(
        config
            .add_section(&id("s2"), "Beds", Some(&id("s1")))
            .is_ok()
    );
}
