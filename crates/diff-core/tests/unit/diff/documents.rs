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
fn docx_table_input_stops_at_the_row_cap() {
    let rows: String = (0..10)
        .map(|index| {
            format!("<w:tr><w:tc><w:p><w:r><w:t>row {index}</w:t></w:r></w:p></w:tc></w:tr>")
        })
        .collect();
    let document = format!(
        "<?xml version=\"1.0\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body><w:tbl>{rows}</w:tbl></w:body></w:document>"
    );
    let bytes = build_zip(&[("word/document.xml", &document)]);
    let package = Package::new(&bytes).unwrap();
    let parsed = parse_docx(&package, 100, 2).unwrap();
    assert!(parsed.truncated);
    assert_eq!(parsed.tables.len(), 1);
    assert_eq!(parsed.tables[0].rows.len(), 2);
}

#[test]
fn follows_presentation_order_and_maps_notes() {
    let presentation = r#"<?xml version="1.0"?>
<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
                xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <p:sldIdLst>
    <p:sldId id="256" r:id="rId2"/>
    <p:sldId id="257" r:id="rId1"/>
  </p:sldIdLst>
</p:presentation>"#;
    let rels = r#"<?xml version="1.0"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide2.xml"/>
</Relationships>"#;
    let slide = |text: &str| {
        format!(
            r#"<?xml version="1.0"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
       xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld><p:spTree><p:sp><p:txBody>
    <a:p><a:r><a:t>{text}</a:t></a:r></a:p>
  </p:txBody></p:sp></p:spTree></p:cSld>
</p:sld>"#
        )
    };
    let notes = r#"<?xml version="1.0"?>
<p:notes xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
  <p:cSld><p:spTree><p:sp><p:txBody>
    <a:p><a:r><a:t>Note for first</a:t></a:r></a:p>
  </p:txBody></p:sp></p:spTree></p:cSld>
</p:notes>"#;
    let slide_rels = r#"<?xml version="1.0"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide" Target="../notesSlides/notesSlide1.xml"/>
</Relationships>"#;
    let slide_one = slide("First slide");
    let slide_two = slide("Second slide");
    let bytes = build_zip(&[
        ("ppt/presentation.xml", presentation),
        ("ppt/_rels/presentation.xml.rels", rels),
        ("ppt/slides/slide1.xml", slide_one.as_str()),
        ("ppt/slides/slide2.xml", slide_two.as_str()),
        ("ppt/slides/_rels/slide1.xml.rels", slide_rels),
        ("ppt/notesSlides/notesSlide1.xml", notes),
    ]);
    let text = extract_pptx_text(&bytes).unwrap();
    let second = text.find("# Slide 1\nSecond slide").unwrap();
    let first = text.find("# Slide 2\nFirst slide").unwrap();
    assert!(second < first, "{text}");
    let note = text.find("# Notes 2\nNote for first").unwrap();
    assert!(note > first, "{text}");
}

#[test]
fn page_insertion_does_not_shift_later_pages() {
    let diff = compare_page_documents(
        "pdf",
        &["Page A".to_string()],
        &["Inserted".to_string(), "Page A".to_string()],
    )
    .unwrap();
    assert_eq!(diff.left_pages, Some(1));
    assert_eq!(diff.right_pages, Some(2));
    assert_eq!(diff.blocks.len(), 1, "{:?}", diff.blocks);
    assert_eq!(diff.blocks[0].change, ChangeKind::Added);
    assert_eq!(diff.blocks[0].path, "page[1]");
}

#[test]
fn compares_docx_paragraphs_by_path() {
    let left = r#"<?xml version="1.0"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>First paragraph</w:t></w:r></w:p>
    <w:p><w:r><w:t>Old value</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let right = r#"<?xml version="1.0"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>First paragraph</w:t></w:r></w:p>
    <w:p><w:r><w:t>New value</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let left_bytes = build_zip(&[("word/document.xml", left)]);
    let right_bytes = build_zip(&[("word/document.xml", right)]);
    let diff = compare_docx_documents(&left_bytes, &right_bytes, 100, 100).unwrap();
    assert_eq!(diff.format, "docx");
    assert_eq!(diff.blocks.len(), 1);
    assert_eq!(diff.blocks[0].path, "body/p[2]");
    assert_eq!(diff.blocks[0].change, ChangeKind::Changed);
    assert_eq!(diff.blocks[0].left.as_deref(), Some("Old value"));
    assert_eq!(diff.blocks[0].right.as_deref(), Some("New value"));
}

#[test]
fn reports_docx_run_formatting_changes() {
    let left = r#"<?xml version="1.0"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body><w:p><w:r><w:t>Hello</w:t></w:r></w:p></w:body>
</w:document>"#;
    let right = r#"<?xml version="1.0"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body><w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Hello</w:t></w:r></w:p></w:body>
</w:document>"#;
    let left_bytes = build_zip(&[("word/document.xml", left)]);
    let right_bytes = build_zip(&[("word/document.xml", right)]);
    let diff = compare_docx_documents(&left_bytes, &right_bytes, 100, 100).unwrap();
    assert_eq!(diff.blocks.len(), 1, "{:?}", diff.blocks);
    assert_eq!(diff.blocks[0].change, ChangeKind::Changed);
    let style = diff.blocks[0].style.as_ref().expect("style change");
    assert_eq!(style.left, None);
    assert_eq!(style.right.as_deref(), Some("b"));
}

#[test]
fn detects_hidden_slides_in_presentation_xml() {
    let xml = r#"<?xml version="1.0"?>
<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
                xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <p:sldIdLst>
    <p:sldId id="256" r:id="rId1" show="0"/>
    <p:sldId id="257" r:id="rId2"/>
  </p:sldIdLst>
</p:presentation>"#;
    let ids = presentation_slide_ids(xml).unwrap();
    assert_eq!(ids.len(), 2);
    assert!(ids[0].1);
    assert!(!ids[1].1);
}

#[test]
fn reports_docx_paragraph_metrics() {
    let left = r#"<?xml version="1.0"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body><w:p><w:r><w:t>Same</w:t></w:r></w:p></w:body>
</w:document>"#;
    let right = r#"<?xml version="1.0"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body><w:p><w:pPr><w:spacing w:before="240" w:line="360" w:lineRule="auto"/><w:jc w:val="center"/></w:pPr><w:r><w:t>Same</w:t></w:r></w:p></w:body>
</w:document>"#;
    let left_bytes = build_zip(&[("word/document.xml", left)]);
    let right_bytes = build_zip(&[("word/document.xml", right)]);
    let diff = compare_docx_documents(&left_bytes, &right_bytes, 100, 100).unwrap();
    let style = diff.blocks[0].style.as_ref().expect("metric change");
    let right_style = style.right.as_deref().unwrap_or_default();
    assert!(right_style.contains("spacing-before:240"), "{right_style}");
    assert!(right_style.contains("align:center"), "{right_style}");
}

#[test]
fn reports_pptx_shape_font_metrics() {
    let slide = |size: &str| {
        format!(
            r#"<?xml version="1.0"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
       xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld><p:spTree><p:sp>
    <p:nvSpPr><p:cNvPr id="1" name="Title 1"/></p:nvSpPr>
    <p:txBody><a:p><a:r><a:rPr sz="{size}" b="1"><a:latin typeface="Arial"/></a:rPr><a:t>Same</a:t></a:r></a:p></p:txBody>
  </p:sp></p:spTree></p:cSld>
</p:sld>"#
        )
    };
    let left = build_zip(&[("ppt/slides/slide1.xml", slide("1800").as_str())]);
    let right = build_zip(&[("ppt/slides/slide1.xml", slide("2400").as_str())]);
    let diff = compare_pptx_documents(&left, &right, 100, 100).unwrap();
    let shape = diff.slides[0]
        .shapes
        .iter()
        .find(|shape| shape.name == "Title 1")
        .expect("shape change");
    let font = shape.font.as_ref().expect("font change");
    assert!(font.left.as_deref().unwrap_or("").contains("sz:18.00pt"));
    assert!(font.right.as_deref().unwrap_or("").contains("sz:24.00pt"));
}

#[test]
fn reports_pptx_shape_geometry_changes() {
    let slide = |x: &str| {
        format!(
            r#"<?xml version="1.0"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
       xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld><p:spTree><p:sp>
    <p:nvSpPr><p:cNvPr id="1" name="Title 1"/></p:nvSpPr>
    <p:spPr><a:xfrm><a:off x="{x}" y="0"/><a:ext cx="100" cy="100"/></a:xfrm></p:spPr>
    <p:txBody><a:p><a:r><a:t>Same text</a:t></a:r></a:p></p:txBody>
  </p:sp></p:spTree></p:cSld>
</p:sld>"#
        )
    };
    let left = build_zip(&[("ppt/slides/slide1.xml", slide("0").as_str())]);
    let right = build_zip(&[("ppt/slides/slide1.xml", slide("50").as_str())]);
    let diff = compare_pptx_documents(&left, &right, 100, 100).unwrap();
    assert_eq!(diff.slides.len(), 1, "{:?}", diff.slides);
    let shape = diff.slides[0]
        .shapes
        .iter()
        .find(|shape| shape.name == "Title 1")
        .expect("shape change");
    let geometry = shape.geometry.as_ref().expect("geometry change");
    assert_eq!(geometry.left.as_deref(), Some("pos(0,0) size(100,100)"));
    assert_eq!(geometry.right.as_deref(), Some("pos(50,0) size(100,100)"));
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
