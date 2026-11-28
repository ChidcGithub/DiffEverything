//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;

#[test]
fn parses_basic_rtf() {
    let value = parse_rtf(br"{\rtf1\ansi Hello \b world\b0!\par Second paragraph.}").unwrap();
    assert_eq!(value["paragraphs"][0], "Hello world!");
    assert_eq!(value["paragraphs"][1], "Second paragraph.");
}

#[test]
fn parses_basic_eml() {
    let message = "From: a@example.com\r\nTo: b@example.com\r\nSubject: Test\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nHello\r\n";
    let value = parse_eml(message.as_bytes()).unwrap();
    assert_eq!(value["headers"]["subject"], "Test");
    assert_eq!(value["text"], "Hello");
}

#[test]
fn rejects_invalid_zip_documents() {
    assert!(parse_odt(b"not a zip").is_err());
    assert!(parse_epub(b"not a zip").is_err());
}
