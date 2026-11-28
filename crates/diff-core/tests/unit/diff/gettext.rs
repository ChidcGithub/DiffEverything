//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;
use std::path::PathBuf;

fn fixture(relative: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.join("fixtures").join(relative)
}

#[test]
fn parses_headers_and_messages() {
    let bytes = std::fs::read(fixture("gettext/test1.mo")).unwrap();
    let value = parse_mo(&bytes).unwrap();
    assert_eq!(value["format"], "gettext");
    assert_eq!(value["charset"], "utf-8");
    assert_eq!(value["headers"]["Project-Id-Version"], "debbindiff");
    assert_eq!(value["messages"][0]["msgid"], "String");
    assert_eq!(value["messages"][0]["msgstr"][0], "String");
}

#[test]
fn decodes_declared_charset() {
    let bytes = std::fs::read(fixture("gettext/test_iso8859-1.mo")).unwrap();
    let value = parse_mo(&bytes).unwrap();
    assert_eq!(value["charset"], "iso8859-1");
    assert!(
        value["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| message["msgstr"][0] == "Chaine, là"),
        "{value:?}"
    );
}

#[test]
fn parses_plural_forms_and_context() {
    let bytes = std::fs::read(fixture("gettext/plural-old.mo")).unwrap();
    let value = parse_mo(&bytes).unwrap();
    let messages = value["messages"].as_array().unwrap();
    let plural = messages
        .iter()
        .find(|message| message["msgid"] == "file")
        .expect("plural message");
    assert_eq!(plural["msgid_plural"], "files");
    assert_eq!(plural["msgstr"][0], "fichier");
    assert_eq!(plural["msgstr"][1], "fichiers");
    let context = messages
        .iter()
        .find(|message| message["msgid"] == "Open")
        .expect("context message");
    assert_eq!(context["context"], "menu");
}

#[test]
fn rejects_truncated_catalog() {
    let bytes = std::fs::read(fixture("gettext/test1.mo")).unwrap();
    assert!(parse_mo(&bytes[..12]).is_err());
}
