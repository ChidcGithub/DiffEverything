//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;

#[test]
fn tokenizes_rust() {
    let source = "fn main() { let value = 1; println!(\"{value}\"); }";
    let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
    let tokens = tokenize(source, &language, TokenizeOptions::default()).unwrap();
    let texts: Vec<&str> = tokens.iter().map(|token| token.text.as_str()).collect();
    assert!(texts.contains(&"fn"));
    assert!(texts.contains(&"main"));
    assert!(texts.contains(&"let"));
    assert!(texts.contains(&"value"));
}

#[test]
fn detects_syntax_change() {
    let left = "fn main() { let value = 1; }";
    let right = "fn main() { let value = 2; }";
    let language = tree_sitter_rust::LANGUAGE.into();
    let left_tokens = tokenize(left, &language, TokenizeOptions::default()).unwrap();
    let right_tokens = tokenize(right, &language, TokenizeOptions::default()).unwrap();
    let diff = compare_token_streams("rust", &left_tokens, &right_tokens, 100);
    assert_eq!(diff.removed_tokens, 1);
    assert_eq!(diff.added_tokens, 1);
    assert!(diff.chunks.iter().any(|chunk| chunk.tag == TextTag::Delete));
    assert!(diff.chunks.iter().any(|chunk| chunk.tag == TextTag::Insert));
    assert!(!diff.truncated);
}

#[test]
fn maps_additional_languages() {
    for (name, expected) in [
        ("script.sh", "bash"),
        ("app.rb", "ruby"),
        ("Program.cs", "csharp"),
        ("main.swift", "swift"),
        ("init.lua", "lua"),
    ] {
        let (language, _) = language_for_name(name).unwrap();
        assert_eq!(language, expected);
    }
}

#[test]
fn ignores_comments_when_requested() {
    let source = "fn main() { /* comment */ let value = 1; }";
    let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
    let tokens = tokenize(
        source,
        &language,
        TokenizeOptions {
            ignore_comments: true,
            max_tokens: 1000,
        },
    )
    .unwrap();
    assert!(!tokens.iter().any(|token| token.kind.contains("comment")));
}
