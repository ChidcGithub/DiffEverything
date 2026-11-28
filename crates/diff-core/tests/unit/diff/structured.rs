//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;

#[test]
fn aligns_inserted_array_items_without_cascading_changes() {
    let left: Value = serde_json::from_str(
        r#"[{"id":1,"name":"alpha"},{"id":2,"name":"beta"},{"id":3,"name":"gamma"}]"#,
    )
    .unwrap();
    let right: Value = serde_json::from_str(
        r#"[{"id":0,"name":"zero"},{"id":1,"name":"alpha"},{"id":2,"name":"beta"},{"id":3,"name":"gamma"}]"#,
    )
    .unwrap();
    let diff = compare_values("json", &left, &right, 100);
    assert_eq!(diff.changes.len(), 1, "{:?}", diff.changes);
    assert_eq!(diff.changes[0].kind, StructuredChangeKind::Added);
    assert_eq!(diff.changes[0].path, "$[0]");
}

#[test]
fn aligns_removed_array_items_without_cascading_changes() {
    let left: Value = serde_json::from_str(r#"["alpha","beta","gamma"]"#).unwrap();
    let right: Value = serde_json::from_str(r#"["alpha","gamma"]"#).unwrap();
    let diff = compare_values("json", &left, &right, 100);
    assert_eq!(diff.changes.len(), 1, "{:?}", diff.changes);
    assert_eq!(diff.changes[0].kind, StructuredChangeKind::Removed);
    assert_eq!(diff.changes[0].left, Some(Value::String("beta".into())));
}

#[test]
fn keeps_in_place_scalar_array_changes() {
    let left: Value = serde_json::from_str(r#"[1,2,3]"#).unwrap();
    let right: Value = serde_json::from_str(r#"[1,9,3]"#).unwrap();
    let diff = compare_values("json", &left, &right, 100);
    assert_eq!(diff.changes.len(), 1, "{:?}", diff.changes);
    assert_eq!(diff.changes[0].path, "$[1]");
}

#[test]
fn reports_array_reordering_instead_of_hiding_it() {
    let left: Value = serde_json::from_str(r#"[{"id":1},{"id":2}]"#).unwrap();
    let right: Value = serde_json::from_str(r#"[{"id":2},{"id":1}]"#).unwrap();
    let diff = compare_values("json", &left, &right, 100);
    assert_eq!(diff.changes.len(), 2, "{:?}", diff.changes);
    assert!(
        diff.changes
            .iter()
            .all(|change| change.kind == StructuredChangeKind::Changed)
    );
    assert!(
        diff.changes
            .iter()
            .any(|change| change.left == Some(Value::from(1))
                && change.right == Some(Value::from(2)))
    );
    assert!(
        diff.changes
            .iter()
            .any(|change| change.left == Some(Value::from(2))
                && change.right == Some(Value::from(1)))
    );
}

#[test]
fn reports_nested_json_paths() {
    let left: Value = serde_json::from_str(r#"{"a":1,"b":[1,2],"c":{"d":true}}"#).unwrap();
    let right: Value = serde_json::from_str(r#"{"a":1,"b":[1,3],"c":{"d":false}}"#).unwrap();
    let diff = compare_values("json", &left, &right, 100);
    let paths: Vec<_> = diff.changes.iter().map(|c| c.path.as_str()).collect();
    assert!(paths.contains(&"$.b[1]"));
    assert!(paths.contains(&"$.c.d"));
}

#[test]
fn reports_added_and_removed_keys() {
    let left: Value = serde_json::from_str(r#"{"a":1,"b":2}"#).unwrap();
    let right: Value = serde_json::from_str(r#"{"a":1,"c":3}"#).unwrap();
    let diff = compare_values("json", &left, &right, 100);
    assert_eq!(diff.changes.len(), 2);
    assert!(
        diff.changes
            .iter()
            .any(|c| c.path == "$.b" && c.kind == StructuredChangeKind::Removed)
    );
    assert!(
        diff.changes
            .iter()
            .any(|c| c.path == "$.c" && c.kind == StructuredChangeKind::Added)
    );
}

#[test]
fn identical_json_has_no_changes() {
    let left: Value = serde_json::from_str(r#"{"a":[1,2]}"#).unwrap();
    let diff = compare_values("json", &left, &left, 100);
    assert!(diff.changes.is_empty());
}

#[test]
fn parses_yaml_and_toml() {
    let yaml = b"name: demo\nitems:\n  - a\n  - b\n";
    let yaml_value = parse_structured(FileKind::Yaml, yaml).unwrap();
    assert_eq!(yaml_value["items"][1], Value::String("b".into()));

    let toml = b"name = \"demo\"\n[server]\nport = 8080\n";
    let toml_value = parse_structured(FileKind::Toml, toml).unwrap();
    assert_eq!(toml_value["server"]["port"], Value::from(8080));

    let ini = b"name = demo\n[server]\nport = 8080\nenabled = true\n";
    let ini_value = parse_structured(FileKind::Ini, ini).unwrap();
    assert_eq!(ini_value["server"]["port"], Value::from(8080));
    assert_eq!(ini_value["server"]["enabled"], Value::Bool(true));
}

#[test]
fn html_formatting_is_semantic() {
    let left = parse_html(b"<html><body><p>Hello <b>world</b></p></body></html>").unwrap();
    let right = parse_html(
        b"<html>\n  <body>\n    <p>\n      Hello\n      <b>world</b>\n    </p>\n  </body>\n</html>",
    )
    .unwrap();
    let diff = compare_values("html", &left, &right, 100);
    assert!(diff.changes.is_empty());
}

#[test]
fn parses_json5() {
    let json5 = b"{name: 'demo', list: [1, 2, 3,], trailing: true}";
    let value = parse_structured(FileKind::Json, json5).unwrap();
    assert_eq!(value["name"], Value::String("demo".into()));
    assert_eq!(value["list"][2], Value::from(3));
}

#[test]
fn parses_xml_and_finds_text_change() {
    let left = parse_xml(b"<root id=\"1\"><item>a</item><item>b</item></root>").unwrap();
    let right = parse_xml(b"<root id=\"1\"><item>a</item><item>c</item></root>").unwrap();
    let diff = compare_values("xml", &left, &right, 100);
    assert!(diff.changes.iter().any(|change| {
        change.path.contains("children[1]") && change.left == Some(Value::String("b".into()))
    }));
}
