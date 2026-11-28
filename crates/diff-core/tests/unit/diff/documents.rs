//! Unit tests for this module, compiled as its child through `#[path]`.

use super::*;
use std::io::Write;
use std::path::PathBuf;

fn fixture(relative: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.join("fixtures").join(relative)
}

#[test]
fn extracts_pdf_text_with_page_markers() {
    let bytes = std::fs::read(fixture("pdf/new.pdf")).unwrap();
    let text = extract_pdf_text(&bytes).unwrap();
    assert!(text.starts_with("# PDF pages: 14"), "{text:.120}");
    assert!(text.contains("# Page 1"));
    assert!(text.contains("# Page 2"));
    assert!(text.contains("Trace-based Just-in-Time"));
}

fn build_zip(files: &[(&str, &str)]) -> Vec<u8> {
    let mut buffer = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buffer));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, data) in files {
            writer.start_file(*name, options).unwrap();
            writer.write_all(data.as_bytes()).unwrap();
        }
        writer.finish().unwrap();
    }
    buffer
}

#[test]
fn extracts_ofd_text() {
    let content = r#"<?xml version="1.0"?>
<ofd:Page xmlns:ofd="http://www.ofdspec.org/2016">
  <ofd:Content>
<ofd:TextObject>
  <ofd:TextCode X="10" Y="10">Hello OFD</ofd:TextCode>
  <ofd:TextCode X="10" Y="20">Second line</ofd:TextCode>
</ofd:TextObject>
  </ofd:Content>
</ofd:Page>"#;
    let bytes = build_zip(&[
        ("OFD.xml", "<ofd:OFD/>"),
        ("Doc_0/Pages/Page_0/Content.xml", content),
    ]);
    let text = extract_ofd_text(&bytes).unwrap();
    assert!(text.contains("Hello OFD"));
    assert!(text.contains("Second line"));
    assert!(text.contains("# Page 1"));
}

#[test]
fn extracts_docx_text_and_headers() {
    let document = r#"<?xml version="1.0"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
<w:p><w:r><w:t>Hello </w:t></w:r><w:r><w:t>world</w:t></w:r></w:p>
<w:p><w:r><w:t>Second paragraph</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let header = r#"<?xml version="1.0"?>
<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:p><w:r><w:t>Confidential header</w:t></w:r></w:p>
</w:hdr>"#;
    let bytes = build_zip(&[
        ("word/document.xml", document),
        ("word/header1.xml", header),
    ]);
    let text = extract_docx_text(&bytes).unwrap();
    assert!(text.contains("Hello world"));
    assert!(text.contains("Second paragraph"));
    assert!(text.contains("Confidential header"));
}

#[test]
fn extracts_pptx_slide_and_notes_text() {
    let slide = r#"<?xml version="1.0"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld><p:spTree><p:sp><p:txBody>
<a:p><a:r><a:t>Slide title</a:t></a:r></a:p>
  </p:txBody></p:sp></p:spTree></p:cSld>
</p:sld>"#;
    let notes = r#"<?xml version="1.0"?>
<p:notes xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
  <p:cSld><p:spTree><p:sp><p:txBody>
<a:p><a:r><a:t>Speaker notes</a:t></a:r></a:p>
  </p:txBody></p:sp></p:spTree></p:cSld>
</p:notes>"#;
    let bytes = build_zip(&[
        ("ppt/slides/slide1.xml", slide),
        ("ppt/notesSlides/notesSlide1.xml", notes),
    ]);
    let text = extract_pptx_text(&bytes).unwrap();
    assert!(text.contains("Slide title"));
    assert!(text.contains("# Slide 1"));
    assert!(text.contains("Speaker notes"));
    assert!(text.contains("# Notes 1"));
}
