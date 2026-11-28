//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;

#[test]
fn parses_supported_remote_schemes() {
    assert!(is_remote(Path::new("http://example.com/a.txt")));
    assert!(is_remote(Path::new("webdav://example.com/a.txt")));
    assert!(is_remote(Path::new("sftp://user@example.com/a.txt")));
    assert!(!is_remote(Path::new("C:\\local\\a.txt")));
}

#[test]
fn rejects_unsafe_remote_file_names() {
    assert!(safe_remote_name("README.md"));
    assert!(safe_remote_name("nested-file.txt"));
    assert!(!safe_remote_name(""));
    assert!(!safe_remote_name("."));
    assert!(!safe_remote_name(".."));
    assert!(!safe_remote_name("../../escape"));
    assert!(!safe_remote_name("..\\escape"));
    assert!(!safe_remote_name("dir/file"));
}

#[test]
fn file_name_uses_url_path() {
    let spec = RemoteSpec::parse("https://example.com/files/old%20file.txt").unwrap();
    assert_eq!(spec.file_name(), "old_20file.txt");
}

#[test]
fn webdav_listing_rejects_foreign_origins_and_siblings() {
    let base = Url::parse("http://example.com/data/left/").unwrap();
    let body = r#"<?xml version="1.0"?><d:multistatus xmlns:d="DAV:">
        <d:response><d:href>/data/left/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop></d:propstat></d:response>
        <d:response><d:href>/data/left/ok.txt</d:href><d:propstat><d:prop><d:resourcetype/></d:prop></d:propstat></d:response>
        <d:response><d:href>/data/right/secret.txt</d:href><d:propstat><d:prop><d:resourcetype/></d:prop></d:propstat></d:response>
        <d:response><d:href>http://evil.example/data/left/steal.txt</d:href><d:propstat><d:prop><d:resourcetype/></d:prop></d:propstat></d:response>
    </d:multistatus>"#;
    let entries = parse_webdav_entries(body, &base).unwrap();
    let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(names, vec!["left", "ok.txt"]);
}
