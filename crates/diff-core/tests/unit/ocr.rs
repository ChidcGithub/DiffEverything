//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;

#[test]
fn missing_binary_is_a_clear_error() {
    let options = OcrOptions::image(
        PathBuf::from("definitely-not-tesseract"),
        None,
        "eng".to_string(),
    );
    let error = ocr_image_bytes(b"not an image", &options).unwrap_err();
    assert!(error.to_string().contains("tesseract not found"));
}
