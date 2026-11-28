//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;

#[test]
fn recognizes_links_and_repository_urls() {
    assert!(is_http_link("https://example.com/file.zip"));
    assert!(is_http_link("http://example.com/file.zip?token=1"));
    assert!(!is_http_link("https://example.com/repo.git"));
    assert!(is_git_url("https://example.com/repo.git"));
    assert!(is_git_url("git@example.com:owner/repo.git"));
    assert!(is_git_url("ssh://git@example.com/repo.git"));
    assert!(is_git_url("file:///tmp/repo.git"));
    assert!(!is_git_url("https://example.com/repo"));
}

#[test]
fn link_file_name_uses_the_last_path_segment() {
    assert_eq!(
        file_name("https://example.com/releases/tool.zip?download=1"),
        "tool.zip"
    );
    assert_eq!(file_name("https://example.com/"), "download.bin");
}

#[test]
fn link_file_name_rejects_traversal_and_reserved_names() {
    assert_eq!(file_name("https://example.com/.."), "download.bin");
    assert_eq!(file_name("https://example.com/%2e%2e"), "download.bin");
    assert_eq!(file_name("https://example.com/NUL"), "download.bin");
    assert_eq!(file_name("https://example.com/com1.txt"), "download.bin");
    assert_eq!(file_name("https://example.com/trailing."), "download.bin");
}

#[cfg(windows)]
#[test]
fn extracts_the_embedded_aria2_binary() {
    let path = aria2_executable(None).unwrap().unwrap();
    assert_eq!(fs::metadata(path).unwrap().len(), ARIA2_SIZE);
}

#[test]
fn parses_content_range_totals() {
    assert_eq!(parse_content_range("bytes 0-0/104857601"), Some(104857601));
    assert_eq!(parse_content_range("bytes 0-0/*"), None);
    assert_eq!(parse_content_range("invalid"), None);
}

#[test]
fn yes_overrides_the_large_download_prompt() {
    assert!(check_size_limit(Some(LARGE_FILE_BYTES), true).is_ok());
    let error = check_size_limit(Some(LARGE_FILE_BYTES + 1), false).unwrap_err();
    assert!(error.to_string().contains("100 MiB"), "{error}");
    assert!(check_size_limit(None, false).is_ok());
}
