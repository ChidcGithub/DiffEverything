use crate::diff::package::Package;
use crate::diff::{table, text};
use crate::model::{
    BlockChange, ChangeKind, DocumentDiff, DocumentTableDiff, FieldChange, MediaChange,
    PresentationDiff, ShapeChange, SlideChange,
};
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// One open ZIP package per input side. DOCX/PPTX/ODT/ODP parsing and media
/// hashing share it, so the central directory is scanned once instead of once
/// per part.
type MediaHashes = BTreeMap<String, (String, u64)>;

pub fn extract_pdf_text(bytes: &[u8]) -> std::result::Result<String, String> {
    if let Ok(Some(text)) = page_aware_pdf_text(bytes) {
        return Ok(text);
    }
    pdf_extract::extract_text_from_mem(bytes).map_err(|error| error.to_string())
}

fn page_aware_pdf_text(bytes: &[u8]) -> std::result::Result<Option<String>, String> {
    let document = lopdf::Document::load_mem(bytes).map_err(|error| error.to_string())?;
    let pages: Vec<u32> = document.get_pages().keys().copied().collect();
    if pages.is_empty() {
        return Ok(None);
    }

    let mut out = format!("# PDF pages: {}\n", pages.len());
    let mut extracted_chars = 0usize;
    for page in pages {
        let mut page_text = String::new();
        for text in document.extract_text_chunks(&[page]).into_iter().flatten() {
            page_text.push_str(&text);
        }
        let page_text = page_text.trim();
        if page_text.is_empty() {
            continue;
        }
        out.push_str(&format!("# Page {page}\n{page_text}\n\n"));
        extracted_chars += page_text.chars().count();
    }

    if extracted_chars == 0 {
        return Ok(None);
    }
    Ok(Some(normalize_text(out)))
}

pub fn extract_ofd_text(bytes: &[u8]) -> std::result::Result<String, String> {
    let package = Package::new(bytes)?;
    extract_ofd_text_package(&package)
}

fn extract_ofd_text_package(package: &Package<'_>) -> std::result::Result<String, String> {
    let mut pages: Vec<&String> = package
        .names
        .iter()
        .filter(|name| name.contains("/Pages/") && name.ends_with(".xml"))
        .collect();
    pages.sort();
    if pages.is_empty() {
        return Err("no OFD page XML found".to_string());
    }

    let mut out = String::new();
    for (index, name) in pages.iter().enumerate() {
        let xml = package.read_text(name)?;
        let text = extract_ofd_xml(&xml)?;
        if !text.trim().is_empty() {
            out.push_str(&format!("# Page {}\n{}\n\n", index + 1, text.trim()));
        }
    }
    if out.trim().is_empty() {
        return Err("OFD contains no extractable text".to_string());
    }
    Ok(normalize_text(out))
}

fn extract_ofd_xml(xml: &str) -> std::result::Result<String, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut out = String::new();
    let mut in_text = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                if event.local_name().as_ref() == "TextCode" {
                    in_text = true;
                }
            }
            Ok(Event::End(event)) => match event.local_name().as_ref() {
                "TextCode" => {
                    in_text = false;
                    out.push('\n');
                }
                "TextObject" => out.push('\n'),
                _ => {}
            },
            Ok(Event::Text(event)) if in_text => {
                out.push_str(&event.xml_content(quick_xml::XmlVersion::Implicit1_0));
            }
            Ok(Event::CData(event)) if in_text => {
                out.push_str(event.as_ref());
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(error.to_string()),
        }
    }

    Ok(out.trim().to_string())
}

pub fn extract_docx_text(bytes: &[u8]) -> std::result::Result<String, String> {
    let package = Package::new(bytes)?;
    extract_docx_text_package(&package)
}

fn extract_docx_text_package(package: &Package<'_>) -> std::result::Result<String, String> {
    let mut part_names: Vec<&String> = package
        .names
        .iter()
        .filter(|name| is_word_text_part(name))
        .collect();
    part_names.sort();
    if !part_names.iter().any(|name| *name == "word/document.xml") {
        return Err("DOCX has no word/document.xml".to_string());
    }

    let mut out = String::new();
    for name in part_names {
        let xml = package.read_text(name)?;
        let text = extract_word_xml(&xml)?;
        if text.trim().is_empty() {
            continue;
        }
        if name != "word/document.xml" {
            let label = name.rsplit('/').next().unwrap_or(name);
            out.push_str(&format!("# {label}\n"));
        }
        out.push_str(&text);
        out.push_str("\n\n");
    }
    Ok(normalize_text(out))
}

pub fn extract_pptx_text(bytes: &[u8]) -> std::result::Result<String, String> {
    let package = Package::new(bytes)?;
    extract_pptx_text_package(&package)
}

fn extract_pptx_text_package(package: &Package<'_>) -> std::result::Result<String, String> {
    let slides = slide_part_order(package)?;
    if slides.is_empty() {
        return Err("no slides found in PPTX".to_string());
    }

    let mut out = String::new();
    for (index, (name, _)) in slides.iter().enumerate() {
        let xml = package.read_text(name)?;
        let text = extract_slide_text(&xml)?;
        // Use the presentation order, not the slideN.xml file number: a
        // reordered deck keeps its file names but changes slide order.
        out.push_str(&format!("# Slide {}\n{text}\n\n", index + 1));
        if let Some(notes_name) = slide_notes_part(package, name)? {
            let notes_xml = package.read_text(&notes_name)?;
            let notes = extract_slide_text(&notes_xml)?;
            if !notes.trim().is_empty() {
                out.push_str(&format!("# Notes {}\n{notes}\n\n", index + 1));
            }
        }
    }
    if out.trim().is_empty() {
        return Err("PPTX contains no extractable slide text".to_string());
    }
    Ok(normalize_text(out))
}

/// Returns slide parts in presentation order, resolved through
/// `ppt/presentation.xml` and its relationships. Falls back to numeric file
/// names when the package does not expose a usable slide list.
fn slide_part_order(package: &Package<'_>) -> std::result::Result<Vec<(String, bool)>, String> {
    if package.contains("ppt/presentation.xml")
        && let Ok(xml) = package.read_text("ppt/presentation.xml")
        && let Ok(ids) = presentation_slide_ids(&xml)
        && !ids.is_empty()
        && package.contains("ppt/_rels/presentation.xml.rels")
        && let Ok(rels_xml) = package.read_text("ppt/_rels/presentation.xml.rels")
    {
        let rels = relationships(&rels_xml)?;
        let targets: std::collections::BTreeMap<String, String> = rels
            .into_iter()
            .filter(|(_, kind, _)| kind.ends_with("/slide"))
            .map(|(id, _, target)| (id, resolve_part_path("ppt", &target)))
            .collect();
        let mut ordered = Vec::new();
        for (id, hidden) in ids {
            if let Some(target) = targets.get(&id)
                && package.contains(target)
            {
                ordered.push((target.clone(), hidden));
            }
        }
        if !ordered.is_empty() {
            // Keep slides whose relationship was not resolvable instead of
            // silently dropping them.
            let mut numbered = numbered_parts(&package.names, "ppt/slides/slide", ".xml");
            numbered.sort_by_key(|(number, _)| *number);
            for (_, name) in numbered {
                if !ordered.iter().any(|(existing, _)| existing == &name) {
                    ordered.push((name, false));
                }
            }
            return Ok(ordered);
        }
    }

    let mut slides = numbered_parts(&package.names, "ppt/slides/slide", ".xml");
    slides.sort_by_key(|(number, _)| *number);
    Ok(slides.into_iter().map(|(_, name)| (name, false)).collect())
}

fn presentation_slide_ids(xml: &str) -> std::result::Result<Vec<(String, bool)>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut in_slide_list = false;
    let mut ids = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) | Ok(Event::Empty(event)) => {
                let name = event.local_name();
                match name.as_ref() {
                    "sldIdLst" => in_slide_list = true,
                    "sldId" if in_slide_list => {
                        if let Some(id) = relationship_id(&event)? {
                            let hidden = attribute_value(&event, "show")
                                .is_some_and(|value| matches!(value.as_str(), "0" | "false"));
                            ids.push((id, hidden));
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(event)) => {
                if event.local_name().as_ref() == "sldIdLst" {
                    in_slide_list = false;
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(ids)
}

fn relationship_id(event: &BytesStart<'_>) -> std::result::Result<Option<String>, String> {
    for attribute in event.attributes().with_checks(false) {
        let attribute = attribute.map_err(|error| error.to_string())?;
        let key = attribute.key.as_ref();
        if key.ends_with(":id") && !key.starts_with("xmlns") {
            let value = attribute
                .normalized_value(XmlVersion::Implicit1_0)
                .map_err(|error| error.to_string())?;
            return Ok(Some(value.into_owned()));
        }
    }
    Ok(None)
}

/// Parses an OPC relationships part into `(id, type, target)` tuples.
fn relationships(xml: &str) -> std::result::Result<Vec<(String, String, String)>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut rels = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) | Ok(Event::Empty(event)) => {
                if event.local_name().as_ref() != "Relationship" {
                    continue;
                }
                let mut id = String::new();
                let mut kind = String::new();
                let mut target = String::new();
                let mut external = false;
                for attribute in event.attributes().with_checks(false) {
                    let attribute = attribute.map_err(|error| error.to_string())?;
                    let value = attribute
                        .normalized_value(XmlVersion::Implicit1_0)
                        .map_err(|error| error.to_string())?
                        .into_owned();
                    match attribute.key.as_ref() {
                        "Id" => id = value,
                        "Type" => kind = value,
                        "Target" => target = value,
                        "TargetMode" => external = value.eq_ignore_ascii_case("External"),
                        _ => {}
                    }
                }
                if !id.is_empty() && !target.is_empty() && !external {
                    rels.push((id, kind, target));
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(rels)
}

fn slide_notes_part(
    package: &Package<'_>,
    slide: &str,
) -> std::result::Result<Option<String>, String> {
    let file = slide.rsplit('/').next().unwrap_or(slide);
    let rels_name = format!("ppt/slides/_rels/{file}.rels");
    if package.contains(&rels_name)
        && let Ok(rels_xml) = package.read_text(&rels_name)
    {
        let rels = relationships(&rels_xml)?;
        if let Some((_, _, target)) = rels
            .into_iter()
            .find(|(_, kind, _)| kind.ends_with("/notesSlide"))
        {
            let target = resolve_part_path("ppt/slides", &target);
            if package.contains(&target) {
                return Ok(Some(target));
            }
        }
    }

    // Minimal decks and hand-built fixtures often omit relationships; keep
    // the old numeric mapping as a fallback.
    let number = slide
        .strip_prefix("ppt/slides/slide")
        .and_then(|rest| rest.strip_suffix(".xml"));
    if let Some(number) = number {
        let fallback = format!("ppt/notesSlides/notesSlide{number}.xml");
        if package.contains(&fallback) {
            return Ok(Some(fallback));
        }
    }
    Ok(None)
}

fn resolve_part_path(base: &str, target: &str) -> String {
    let absolute = target.starts_with('/');
    let mut parts: Vec<&str> = if absolute {
        Vec::new()
    } else {
        base.split('/').filter(|part| !part.is_empty()).collect()
    };
    for part in target.trim_start_matches('/').split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

#[derive(Debug)]
struct PdfParsed {
    pages: Vec<String>,
    blocks: Vec<ParsedBlock>,
    metadata: BTreeMap<String, String>,
}

pub fn compare_pdf_documents(
    left: &[u8],
    right: &[u8],
) -> std::result::Result<DocumentDiff, String> {
    // Parse both sides concurrently, but keep one Document per side. The old
    // implementation loaded each PDF three times (text, blocks and metadata).
    let (left_parsed, right_parsed) = rayon::join(|| parse_pdf(left), || parse_pdf(right));
    let left_parsed = left_parsed?;
    let right_parsed = right_parsed?;
    let left_blocks = if left_parsed.blocks.is_empty() {
        pages_to_blocks(&left_parsed.pages)
    } else {
        left_parsed.blocks
    };
    let right_blocks = if right_parsed.blocks.is_empty() {
        pages_to_blocks(&right_parsed.pages)
    } else {
        right_parsed.blocks
    };
    let mut diff = compare_page_blocks(
        "pdf",
        &left_parsed.pages,
        &right_parsed.pages,
        left_blocks,
        right_blocks,
    )?;
    diff.metadata.extend(compare_metadata(
        &left_parsed.metadata,
        &right_parsed.metadata,
    ));
    Ok(diff)
}

fn parse_pdf(bytes: &[u8]) -> std::result::Result<PdfParsed, String> {
    let document = lopdf::Document::load_mem(bytes).map_err(|error| error.to_string())?;
    let mut pages = Vec::new();
    let mut blocks = Vec::new();
    let mut metadata = pdf_info_metadata(&document);
    let mut page_numbers = Vec::new();

    for (page_number, page_id) in document.get_pages() {
        page_numbers.push(page_number);

        let mut text = String::new();
        for chunk in document
            .extract_text_chunks(&[page_number])
            .into_iter()
            .flatten()
        {
            text.push_str(&chunk);
        }
        pages.push(normalize_text(text));

        let content = document.get_page_content(page_id);
        if let Ok(content) = lopdf::content::Content::decode(&content) {
            let mut x = 0.0f32;
            let mut y = 0.0f32;
            let mut block_index = 0usize;
            for operation in content.operations {
                match operation.operator.as_str() {
                    "Tm" => {
                        if let Some(values) = operation_numbers(&operation.operands, 6) {
                            x = values[4];
                            y = values[5];
                        }
                    }
                    "Td" => {
                        if let Some(values) = operation_numbers(&operation.operands, 2) {
                            x += values[0];
                            y += values[1];
                        }
                    }
                    "Tj" | "'" => {
                        if let Some(text) = operation_text(&operation.operands) {
                            push_positioned_block(
                                &mut blocks,
                                page_number,
                                &mut block_index,
                                text,
                                x,
                                y,
                            );
                        }
                    }
                    "TJ" => {
                        if let Some(text) = operation_array_text(&operation.operands) {
                            push_positioned_block(
                                &mut blocks,
                                page_number,
                                &mut block_index,
                                text,
                                x,
                                y,
                            );
                        }
                    }
                    _ => {}
                }
            }
        }

        let Ok(page) = document
            .get_object(page_id)
            .and_then(|object| object.as_dict())
        else {
            continue;
        };
        let Some(media_box) = page.get(b"MediaBox").ok().and_then(|object| {
            if let Ok(id) = object.as_reference() {
                document
                    .get_object(id)
                    .ok()
                    .and_then(|object| object.as_array().ok())
            } else {
                object.as_array().ok()
            }
        }) else {
            continue;
        };
        let numbers: Vec<String> = media_box
            .iter()
            .filter_map(|value| value.as_float().ok().map(|value| format!("{value:.1}")))
            .collect();
        if numbers.len() == 4 {
            metadata.insert(
                format!("page[{page_number}]/size"),
                format!("{}x{}", numbers[2], numbers[3]),
            );
        }
    }

    if page_numbers.is_empty() {
        return Err("PDF has no pages".to_string());
    }
    if pages.iter().all(|text| text.is_empty()) {
        return Err("PDF has no extractable text".to_string());
    }
    Ok(PdfParsed {
        pages,
        blocks,
        metadata,
    })
}

fn pdf_info_metadata(document: &lopdf::Document) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    if let Ok(info) = document.trailer.get(b"Info") {
        let dictionary = info
            .as_reference()
            .ok()
            .and_then(|id| document.get_object(id).ok())
            .and_then(|object| object.as_dict().ok())
            .or_else(|| info.as_dict().ok());
        if let Some(dictionary) = dictionary {
            let keys: [&[u8]; 7] = [
                b"Title",
                b"Author",
                b"Subject",
                b"Creator",
                b"Producer",
                b"CreationDate",
                b"ModDate",
            ];
            for key in keys {
                if let Ok(value) = dictionary.get(key)
                    && let Ok(text) = value.as_str()
                {
                    values.insert(
                        format!("info/{}", String::from_utf8_lossy(key)),
                        decode_pdf_string(text),
                    );
                }
            }
        }
    }
    values
}

fn push_positioned_block(
    blocks: &mut Vec<ParsedBlock>,
    page_number: u32,
    block_index: &mut usize,
    text: String,
    x: f32,
    y: f32,
) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    *block_index += 1;
    blocks.push(ParsedBlock {
        path: format!("page[{page_number}]/block[{block_index}]"),
        kind: "text".to_string(),
        text: text.to_string(),
        style: None,
        position: Some(format!("x={x:.1},y={y:.1}")),
    });
}

fn operation_numbers(operands: &[lopdf::Object], count: usize) -> Option<Vec<f32>> {
    if operands.len() < count {
        return None;
    }
    operands[..count]
        .iter()
        .map(|object| object.as_float().ok())
        .collect()
}

fn operation_text(operands: &[lopdf::Object]) -> Option<String> {
    let first = operands.first()?;
    match first {
        lopdf::Object::String(bytes, _) => Some(decode_pdf_string(bytes)),
        _ => None,
    }
}

fn operation_array_text(operands: &[lopdf::Object]) -> Option<String> {
    let lopdf::Object::Array(items) = operands.first()? else {
        return None;
    };
    let mut text = String::new();
    for item in items {
        if let lopdf::Object::String(bytes, _) = item {
            text.push_str(&decode_pdf_string(bytes));
        }
    }
    (!text.is_empty()).then_some(text)
}

pub fn compare_mixed_documents(
    left_kind: crate::model::FileKind,
    left: &[u8],
    right_kind: crate::model::FileKind,
    right: &[u8],
    max_blocks: usize,
    max_table_rows: usize,
) -> std::result::Result<DocumentDiff, String> {
    let left_doc = parse_mixed_side(left_kind, left, max_blocks)?;
    let right_doc = parse_mixed_side(right_kind, right, max_blocks)?;
    if left_doc.blocks.is_empty()
        && left_doc.tables.is_empty()
        && right_doc.blocks.is_empty()
        && right_doc.tables.is_empty()
    {
        return Err("no comparable document blocks".to_string());
    }
    Ok(DocumentDiff {
        format: format!("{}-to-{}", left_kind.label(), right_kind.label()),
        left_pages: None,
        right_pages: None,
        metadata: compare_metadata(&left_doc.metadata, &right_doc.metadata),
        blocks: compare_blocks(&left_doc.blocks, &right_doc.blocks),
        tables: compare_docx_tables(&left_doc.tables, &right_doc.tables, max_table_rows),
        media: Vec::new(),
        visual: None,
        truncated: left_doc.truncated || right_doc.truncated,
    })
}

struct MixedDocument {
    blocks: Vec<ParsedBlock>,
    tables: Vec<ParsedTableRows>,
    metadata: BTreeMap<String, String>,
    truncated: bool,
}

fn parse_mixed_side(
    kind: crate::model::FileKind,
    bytes: &[u8],
    max_blocks: usize,
) -> std::result::Result<MixedDocument, String> {
    match kind {
        crate::model::FileKind::Docx => {
            let package = Package::new(bytes)?;
            let parsed = parse_docx(&package, max_blocks)?;
            Ok(MixedDocument {
                blocks: parsed.blocks,
                tables: parsed.tables,
                metadata: BTreeMap::new(),
                truncated: parsed.truncated,
            })
        }
        crate::model::FileKind::Odt => {
            let package = Package::new(bytes)?;
            let parsed = parse_odf_document(&package, "odt", max_blocks)?;
            Ok(MixedDocument {
                blocks: parsed.blocks,
                tables: parsed.tables,
                metadata: parsed.metadata,
                truncated: parsed.truncated,
            })
        }
        crate::model::FileKind::Pdf => {
            let parsed = parse_pdf(bytes)?;
            let mut blocks = if parsed.blocks.is_empty() {
                pages_to_blocks(&parsed.pages)
            } else {
                parsed.blocks
            };
            let truncated = blocks.len() > max_blocks;
            blocks.truncate(max_blocks);
            Ok(MixedDocument {
                blocks,
                tables: Vec::new(),
                metadata: parsed.metadata,
                truncated,
            })
        }
        crate::model::FileKind::Ofd => {
            let pages = ofd_page_texts(bytes)?;
            let mut blocks = pages_to_blocks(&pages);
            let truncated = blocks.len() > max_blocks;
            blocks.truncate(max_blocks);
            Ok(MixedDocument {
                blocks,
                tables: Vec::new(),
                metadata: BTreeMap::new(),
                truncated,
            })
        }
        crate::model::FileKind::Pptx => {
            let package = Package::new(bytes)?;
            let (slides, truncated) = parse_pptx_slides(&package, max_blocks)?;
            let mut blocks = Vec::new();
            let mut tables = Vec::new();
            for (index, slide) in slides.iter().enumerate() {
                let mut paragraph_index = 0usize;
                let mut table_index = 0usize;
                for shape in &slide.shapes {
                    if !shape.text.trim().is_empty() {
                        if blocks.len() >= max_blocks {
                            return Ok(MixedDocument {
                                blocks,
                                tables,
                                metadata: BTreeMap::new(),
                                truncated: true,
                            });
                        }
                        paragraph_index += 1;
                        blocks.push(ParsedBlock {
                            path: format!("slide[{}]/p[{paragraph_index}]", index + 1),
                            kind: "paragraph".to_string(),
                            text: shape.text.clone(),
                            style: None,
                            position: None,
                        });
                    }
                    if let Some(rows) = &shape.table
                        && !rows.is_empty()
                    {
                        table_index += 1;
                        tables.push(ParsedTableRows {
                            path: format!("slide[{}]/tbl[{table_index}]", index + 1),
                            rows: rows.clone(),
                        });
                    }
                }
            }
            Ok(MixedDocument {
                blocks,
                tables,
                metadata: BTreeMap::new(),
                truncated,
            })
        }
        crate::model::FileKind::Odp => {
            let package = Package::new(bytes)?;
            let parsed = parse_odf_document(&package, "odp", max_blocks)?;
            Ok(MixedDocument {
                blocks: parsed.blocks,
                tables: parsed.tables,
                metadata: parsed.metadata,
                truncated: parsed.truncated,
            })
        }
        other => Err(format!(
            "no block mapper for {} in cross-format mode",
            other.label()
        )),
    }
}

pub fn compare_odf_documents(
    left: &[u8],
    right: &[u8],
    format: &str,
    max_blocks: usize,
    max_table_rows: usize,
) -> std::result::Result<DocumentDiff, String> {
    let (left_result, right_result) = rayon::join(
        || -> std::result::Result<(ParsedOdf, MediaHashes), String> {
            let package = Package::new(left)?;
            let parsed = parse_odf_document(&package, format, max_blocks)?;
            let media = media_hashes(&package, "Pictures/");
            Ok((parsed, media))
        },
        || -> std::result::Result<(ParsedOdf, MediaHashes), String> {
            let package = Package::new(right)?;
            let parsed = parse_odf_document(&package, format, max_blocks)?;
            let media = media_hashes(&package, "Pictures/");
            Ok((parsed, media))
        },
    );
    let (left_doc, left_media) = left_result?;
    let (right_doc, right_media) = right_result?;
    Ok(DocumentDiff {
        format: format.to_string(),
        left_pages: None,
        right_pages: None,
        metadata: compare_metadata(&left_doc.metadata, &right_doc.metadata),
        blocks: compare_blocks(&left_doc.blocks, &right_doc.blocks),
        tables: compare_docx_tables(&left_doc.tables, &right_doc.tables, max_table_rows),
        media: compare_media_maps(&left_media, &right_media, "image"),
        visual: None,
        truncated: left_doc.truncated || right_doc.truncated,
    })
}

struct ParsedOdf {
    blocks: Vec<ParsedBlock>,
    tables: Vec<ParsedTableRows>,
    metadata: BTreeMap<String, String>,
    truncated: bool,
}

fn parse_odf_document(
    package: &Package<'_>,
    format: &str,
    max_blocks: usize,
) -> std::result::Result<ParsedOdf, String> {
    if !package.contains("content.xml") {
        return Err(format!("{format}: missing content.xml"));
    }
    let xml = package.read_text("content.xml")?;
    let mut parsed = ParsedOdf {
        blocks: Vec::new(),
        tables: Vec::new(),
        metadata: odf_metadata(package),
        truncated: false,
    };
    parse_odf_content(&xml, format, max_blocks, &mut parsed)?;
    Ok(parsed)
}

fn parse_odf_content(
    xml: &str,
    format: &str,
    max_blocks: usize,
    parsed: &mut ParsedOdf,
) -> std::result::Result<(), String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut in_paragraph = false;
    let mut in_heading = false;
    let mut in_table = false;
    let mut in_cell = false;
    let mut paragraph_index = 0usize;
    let mut table_index = 0usize;
    let mut slide_index = 0usize;
    let mut current_text = String::new();
    let mut current_style = String::new();
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cell = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => match event.local_name().as_ref() {
                "page" if format == "odp" => {
                    slide_index += 1;
                    paragraph_index = 0;
                }
                "p" | "h" if !in_table => {
                    in_paragraph = true;
                    in_heading = event.local_name().as_ref() == "h";
                    current_text.clear();
                    current_style.clear();
                    if let Some(style) = attribute_value(&event, "style-name") {
                        current_style = style;
                    }
                }
                "table" if !in_table => {
                    in_table = true;
                    table_rows.clear();
                }
                "table-row" if in_table => row.clear(),
                "table-cell" if in_table => {
                    cell.clear();
                    in_cell = true;
                }
                "tab" if in_cell => cell.push('\t'),
                "line-break" if in_cell => cell.push('\n'),
                "s" if in_cell => cell.push(' '),
                "tab" if in_paragraph => current_text.push('\t'),
                "line-break" if in_paragraph => current_text.push('\n'),
                "s" if in_paragraph => current_text.push(' '),
                _ => {}
            },
            Ok(Event::Empty(event)) => match event.local_name().as_ref() {
                "tab" if in_cell => cell.push('\t'),
                "tab" if in_paragraph => current_text.push('\t'),
                "line-break" if in_cell => cell.push('\n'),
                "line-break" if in_paragraph => current_text.push('\n'),
                "s" if in_cell => cell.push(' '),
                "s" if in_paragraph => current_text.push(' '),
                _ => {}
            },
            Ok(Event::Text(event)) => {
                let value = event.xml_content(XmlVersion::Implicit1_0);
                if in_table {
                    if in_cell {
                        cell.push_str(&value);
                    }
                } else if in_paragraph {
                    current_text.push_str(&value);
                }
            }
            Ok(Event::CData(event)) => {
                if in_table {
                    if in_cell {
                        cell.push_str(event.as_ref());
                    }
                } else if in_paragraph {
                    current_text.push_str(event.as_ref());
                }
            }
            Ok(Event::End(event)) => match event.local_name().as_ref() {
                "p" | "h" if !in_table => {
                    let text = current_text.trim().to_string();
                    if !text.is_empty() {
                        paragraph_index += 1;
                        if parsed.blocks.len() < max_blocks {
                            let path = if format == "odp" {
                                format!("slide[{slide_index}]/p[{paragraph_index}]")
                            } else {
                                format!("body/p[{paragraph_index}]")
                            };
                            let mut style = current_style.clone();
                            if in_heading {
                                style = format!("heading {style}").trim().to_string();
                            }
                            parsed.blocks.push(ParsedBlock {
                                path,
                                kind: if in_heading {
                                    "heading".to_string()
                                } else {
                                    "paragraph".to_string()
                                },
                                text,
                                style: (!style.is_empty()).then_some(style),
                                position: None,
                            });
                        } else {
                            parsed.truncated = true;
                        }
                    }
                    in_paragraph = false;
                    in_heading = false;
                }
                "p" if in_table && in_cell && !cell.ends_with('\n') => cell.push('\n'),
                "table-cell" if in_table => {
                    row.push(cell.trim().to_string());
                    in_cell = false;
                }
                "table-row" if in_table => {
                    if !row.is_empty() {
                        table_rows.push(std::mem::take(&mut row));
                    }
                }
                "table" if in_table => {
                    in_table = false;
                    table_index += 1;
                    let path = if format == "odp" {
                        format!("slide[{slide_index}]/tbl[{table_index}]")
                    } else {
                        format!("body/tbl[{table_index}]")
                    };
                    if !table_rows.is_empty() {
                        let columns = table_rows.iter().map(Vec::len).max().unwrap_or(0);
                        if parsed.blocks.len() < max_blocks {
                            parsed.blocks.push(ParsedBlock {
                                path: path.clone(),
                                kind: "table".to_string(),
                                text: format!(
                                    "table {} row(s) x {columns} column(s)",
                                    table_rows.len()
                                ),
                                style: None,
                                position: None,
                            });
                        } else {
                            parsed.truncated = true;
                        }
                        parsed.tables.push(ParsedTableRows {
                            path,
                            rows: std::mem::take(&mut table_rows),
                        });
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

fn odf_metadata(package: &Package<'_>) -> BTreeMap<String, String> {
    let Ok(xml) = package.read_text("meta.xml") else {
        return BTreeMap::new();
    };
    crate::diff::documents_extra::metadata_fields(xml.as_bytes())
        .ok()
        .and_then(|value| value.as_object().cloned())
        .map(|object| {
            object
                .into_iter()
                .filter_map(|(key, value)| value.as_str().map(|value| (key, value.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

fn compare_metadata(
    left: &BTreeMap<String, String>,
    right: &BTreeMap<String, String>,
) -> Vec<FieldChange> {
    let keys: BTreeSet<&String> = left.keys().chain(right.keys()).collect();
    keys.into_iter()
        .filter(|key| left.get(*key) != right.get(*key))
        .map(|key| FieldChange {
            path: format!("metadata/{key}"),
            left: left.get(key).cloned(),
            right: right.get(key).cloned(),
        })
        .collect()
}

pub fn compare_ofd_documents(
    left: &[u8],
    right: &[u8],
) -> std::result::Result<DocumentDiff, String> {
    let left_pages = ofd_page_texts(left)?;
    let right_pages = ofd_page_texts(right)?;
    compare_page_documents("ofd", &left_pages, &right_pages)
}

fn compare_page_documents(
    format: &str,
    left_pages: &[String],
    right_pages: &[String],
) -> std::result::Result<DocumentDiff, String> {
    let left_blocks = pages_to_blocks(left_pages);
    let right_blocks = pages_to_blocks(right_pages);
    compare_page_blocks(format, left_pages, right_pages, left_blocks, right_blocks)
}

fn compare_page_blocks(
    format: &str,
    left_pages: &[String],
    right_pages: &[String],
    left_blocks: Vec<ParsedBlock>,
    right_blocks: Vec<ParsedBlock>,
) -> std::result::Result<DocumentDiff, String> {
    Ok(DocumentDiff {
        format: format.to_string(),
        left_pages: Some(left_pages.len()),
        right_pages: Some(right_pages.len()),
        metadata: Vec::new(),
        blocks: compare_blocks(&left_blocks, &right_blocks),
        tables: Vec::new(),
        media: Vec::new(),
        visual: None,
        truncated: false,
    })
}

fn pages_to_blocks(pages: &[String]) -> Vec<ParsedBlock> {
    pages
        .iter()
        .enumerate()
        .map(|(index, text)| ParsedBlock {
            path: format!("page[{}]", index + 1),
            kind: "page".to_string(),
            text: text.clone(),
            style: None,
            position: None,
        })
        .collect()
}

fn decode_pdf_string(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return encoding_rs::UTF_16BE.decode(rest).0.into_owned();
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return encoding_rs::UTF_16LE.decode(rest).0.into_owned();
    }
    String::from_utf8_lossy(bytes).to_string()
}

fn ofd_page_texts(bytes: &[u8]) -> std::result::Result<Vec<String>, String> {
    let package = Package::new(bytes)?;
    ofd_page_texts_package(&package)
}

fn ofd_page_texts_package(package: &Package<'_>) -> std::result::Result<Vec<String>, String> {
    let mut pages: Vec<&String> = package
        .names
        .iter()
        .filter(|name| name.contains("/Pages/") && name.ends_with(".xml"))
        .collect();
    pages.sort();
    if pages.is_empty() {
        return Err("no OFD page XML found".to_string());
    }
    let mut texts = Vec::with_capacity(pages.len());
    for name in pages {
        let xml = package.read_text(name)?;
        texts.push(normalize_text(extract_ofd_xml(&xml)?));
    }
    if texts.iter().all(|text| text.is_empty()) {
        return Err("OFD contains no extractable text".to_string());
    }
    Ok(texts)
}

#[derive(Debug, Clone)]
struct ParsedBlock {
    path: String,
    kind: String,
    text: String,
    style: Option<String>,
    position: Option<String>,
}

#[derive(Debug, Clone)]
struct ParsedTableRows {
    path: String,
    rows: Vec<Vec<String>>,
}

#[derive(Debug, Default)]
struct ParsedPart {
    blocks: Vec<ParsedBlock>,
    tables: Vec<ParsedTableRows>,
    truncated: bool,
}

#[derive(Debug, Default)]
struct ParsedDocx {
    blocks: Vec<ParsedBlock>,
    tables: Vec<ParsedTableRows>,
    truncated: bool,
}

/// Compares two DOCX packages at paragraph/table granularity instead of
/// flattening the whole body into one text blob.
pub fn compare_docx_documents(
    left: &[u8],
    right: &[u8],
    max_blocks: usize,
    max_table_rows: usize,
) -> std::result::Result<DocumentDiff, String> {
    let (left_result, right_result) = rayon::join(
        || -> std::result::Result<(ParsedDocx, MediaHashes), String> {
            let package = Package::new(left)?;
            let parsed = parse_docx(&package, max_blocks)?;
            let media = media_hashes(&package, "word/media/");
            Ok((parsed, media))
        },
        || -> std::result::Result<(ParsedDocx, MediaHashes), String> {
            let package = Package::new(right)?;
            let parsed = parse_docx(&package, max_blocks)?;
            let media = media_hashes(&package, "word/media/");
            Ok((parsed, media))
        },
    );
    let (left_doc, left_media) = left_result?;
    let (right_doc, right_media) = right_result?;
    let blocks = compare_blocks(&left_doc.blocks, &right_doc.blocks);
    let tables = compare_docx_tables(&left_doc.tables, &right_doc.tables, max_table_rows);
    let media = compare_media_maps(&left_media, &right_media, "image");
    Ok(DocumentDiff {
        format: "docx".to_string(),
        left_pages: None,
        right_pages: None,
        metadata: Vec::new(),
        blocks,
        tables,
        media,
        visual: None,
        truncated: left_doc.truncated || right_doc.truncated,
    })
}

fn parse_docx(package: &Package<'_>, max_blocks: usize) -> std::result::Result<ParsedDocx, String> {
    if !package.contains("word/document.xml") {
        return Err("DOCX has no word/document.xml".to_string());
    }
    let mut parsed = ParsedDocx::default();
    let xml = package.read_text("word/document.xml")?;
    let part = parse_word_part(&xml, "body", max_blocks)?;
    parsed.blocks.extend(part.blocks);
    parsed.tables.extend(part.tables);
    parsed.truncated |= part.truncated;

    let mut parts: Vec<(String, String)> = Vec::new();
    for (prefix, label) in [("word/header", "header"), ("word/footer", "footer")] {
        let mut numbered = numbered_parts(&package.names, prefix, ".xml");
        numbered.sort_by_key(|(number, _)| *number);
        for (position, (_, name)) in numbered.into_iter().enumerate() {
            parts.push((name, format!("{label}[{}]", position + 1)));
        }
    }
    for (name, label) in [
        ("word/footnotes.xml", "footnote"),
        ("word/endnotes.xml", "endnote"),
        ("word/comments.xml", "comment"),
    ] {
        if package.contains(name) {
            parts.push((name.to_string(), label.to_string()));
        }
    }

    for (name, prefix) in parts {
        if parsed.truncated {
            break;
        }
        let xml = package.read_text(&name)?;
        let remaining = max_blocks.saturating_sub(parsed.blocks.len());
        let part = parse_word_part(&xml, &prefix, remaining)?;
        parsed.blocks.extend(part.blocks);
        parsed.tables.extend(part.tables);
        parsed.truncated |= part.truncated;
    }
    Ok(parsed)
}

fn parse_word_part(
    xml: &str,
    prefix: &str,
    max_blocks: usize,
) -> std::result::Result<ParsedPart, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut parsed = ParsedPart::default();
    let mut paragraph_index = 0usize;
    let mut table_index = 0usize;
    let mut in_text = false;
    let mut in_table = false;
    let mut in_cell = false;
    let mut current_text = String::new();
    let mut current_kind = "paragraph".to_string();
    let mut current_style = String::new();
    let mut current_format: BTreeSet<String> = BTreeSet::new();
    let mut current_run_format: BTreeSet<String> = BTreeSet::new();
    let mut in_run_props = false;
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cell = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => match event.local_name().as_ref() {
                "tbl" if !in_table => {
                    in_table = true;
                    table_rows.clear();
                    row.clear();
                    cell.clear();
                }
                "tr" if in_table => row.clear(),
                "tc" if in_table => {
                    cell.clear();
                    in_cell = true;
                }
                "p" if !in_table => {
                    current_text.clear();
                    current_kind = "paragraph".to_string();
                    current_style.clear();
                    current_format.clear();
                }
                "pStyle" => {
                    if let Some(value) = attribute_value(&event, "val") {
                        current_style = value;
                    }
                }
                "numPr" => current_kind = "list-item".to_string(),
                "outlineLvl" => {
                    if attribute_value(&event, "val").as_deref() == Some("0") {
                        current_kind = "heading".to_string();
                    }
                }
                "r" if !in_table => {
                    current_run_format.clear();
                }
                "rPr" if !in_table => {
                    in_run_props = true;
                    current_run_format.clear();
                }
                "spacing" | "ind" if !in_run_props && !in_table => apply_paragraph_property(
                    event.local_name().as_ref(),
                    &event,
                    &mut current_format,
                ),
                "jc" | "keepNext" | "keepLines" | "pageBreakBefore" | "widowControl"
                    if !in_run_props && !in_table =>
                {
                    apply_paragraph_property(
                        event.local_name().as_ref(),
                        &event,
                        &mut current_format,
                    )
                }
                local if in_run_props && !in_table => {
                    apply_run_property(local, &event, &mut current_run_format)
                }
                "t" => in_text = true,
                "tab" | "br" | "cr" if !in_table => {
                    push_break(&mut current_text, event.local_name().as_ref())
                }
                _ => {}
            },
            Ok(Event::Empty(event)) => match event.local_name().as_ref() {
                "spacing" | "ind" | "jc" | "keepNext" | "keepLines" | "pageBreakBefore"
                | "widowControl"
                    if !in_run_props && !in_table =>
                {
                    apply_paragraph_property(
                        event.local_name().as_ref(),
                        &event,
                        &mut current_format,
                    )
                }
                local if in_run_props && !in_table => {
                    apply_run_property(local, &event, &mut current_run_format)
                }
                "tab" | "br" | "cr" if !in_table => {
                    push_break(&mut current_text, event.local_name().as_ref())
                }
                "tab" | "br" | "cr" if in_table && in_cell => {
                    push_break(&mut cell, event.local_name().as_ref())
                }
                _ => {}
            },
            Ok(Event::Text(event)) if in_text => {
                let value = event.xml_content(XmlVersion::Implicit1_0);
                if in_table {
                    if in_cell {
                        cell.push_str(&value);
                    }
                } else {
                    current_text.push_str(&value);
                }
            }
            Ok(Event::CData(event)) if in_text => {
                if in_table {
                    if in_cell {
                        cell.push_str(event.as_ref());
                    }
                } else {
                    current_text.push_str(event.as_ref());
                }
            }
            Ok(Event::End(event)) => match event.local_name().as_ref() {
                "rPr" if !in_table => in_run_props = false,
                "r" if !in_table => {
                    current_format.extend(current_run_format.iter().cloned());
                    current_run_format.clear();
                    in_run_props = false;
                }
                "t" => in_text = false,
                "tc" if in_table => {
                    row.push(cell.trim().to_string());
                    in_cell = false;
                }
                "tr" if in_table => {
                    if !row.is_empty() {
                        table_rows.push(std::mem::take(&mut row));
                    }
                }
                "tbl" if in_table => {
                    in_table = false;
                    table_index += 1;
                    let path = format!("{prefix}/tbl[{table_index}]");
                    if !table_rows.is_empty() {
                        let columns = table_rows.iter().map(Vec::len).max().unwrap_or(0);
                        if parsed.blocks.len() < max_blocks {
                            parsed.blocks.push(ParsedBlock {
                                path: path.clone(),
                                kind: "table".to_string(),
                                text: format!(
                                    "table {} row(s) x {columns} column(s)",
                                    table_rows.len()
                                ),
                                style: None,
                                position: None,
                            });
                        } else {
                            parsed.truncated = true;
                        }
                        parsed.tables.push(ParsedTableRows {
                            path,
                            rows: std::mem::take(&mut table_rows),
                        });
                    }
                }
                "p" if !in_table => {
                    let text = current_text.trim().to_string();
                    if !text.is_empty() {
                        paragraph_index += 1;
                        if parsed.blocks.len() < max_blocks {
                            let kind = if current_kind == "paragraph"
                                && current_style.to_ascii_lowercase().contains("heading")
                            {
                                "heading".to_string()
                            } else {
                                current_kind.clone()
                            };
                            let mut style_parts = Vec::new();
                            if !current_style.is_empty() {
                                style_parts.push(format!("style={current_style}"));
                            }
                            style_parts.extend(current_format.iter().cloned());
                            let style = (!style_parts.is_empty()).then(|| style_parts.join(";"));
                            parsed.blocks.push(ParsedBlock {
                                path: format!("{prefix}/p[{paragraph_index}]"),
                                kind,
                                text,
                                style,
                                position: None,
                            });
                        } else {
                            parsed.truncated = true;
                        }
                    }
                }
                "p" if in_table && in_cell && !cell.ends_with('\n') => {
                    cell.push('\n');
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(parsed)
}

fn apply_paragraph_property(local_name: &str, event: &BytesStart<'_>, tags: &mut BTreeSet<String>) {
    match local_name {
        "spacing" => {
            for key in [
                "before",
                "after",
                "line",
                "lineRule",
                "beforeLines",
                "afterLines",
            ] {
                if let Some(value) = attribute_value(event, key) {
                    tags.insert(format!("spacing-{key}:{value}"));
                }
            }
        }
        "ind" => {
            for key in ["left", "right", "firstLine", "hanging", "start", "end"] {
                if let Some(value) = attribute_value(event, key) {
                    tags.insert(format!("indent-{key}:{value}"));
                }
            }
        }
        "jc" => {
            if let Some(value) = attribute_value(event, "val") {
                tags.insert(format!("align:{value}"));
            }
        }
        "keepNext" | "keepLines" | "pageBreakBefore" | "widowControl" => {
            tags.insert(local_name.to_string());
        }
        _ => {}
    }
}

fn apply_ppt_run_attributes(event: &BytesStart<'_>, tags: &mut BTreeSet<String>) {
    for key in ["b", "i", "u", "strike"] {
        if let Some(value) = attribute_value(event, key) {
            if matches!(value.as_str(), "0" | "false" | "off") {
                tags.insert(format!("no-{key}"));
            } else {
                tags.insert(key.to_string());
            }
        }
    }
    if let Some(value) = attribute_value(event, "sz")
        && let Ok(size) = value.parse::<u32>()
    {
        tags.insert(format!("sz:{:.2}pt", size as f64 / 100.0));
    }
    for key in ["baseline", "spc"] {
        if let Some(value) = attribute_value(event, key) {
            tags.insert(format!("{key}:{value}"));
        }
    }
}

fn apply_run_property(local_name: &str, event: &BytesStart<'_>, tags: &mut BTreeSet<String>) {
    match local_name {
        "b" | "i" | "strike" | "dstrike" | "caps" | "smallCaps" => {
            let enabled = attribute_value(event, "val")
                .map(|value| !matches!(value.as_str(), "0" | "false" | "off"))
                .unwrap_or(true);
            if enabled {
                tags.insert(local_name.to_string());
            } else {
                tags.insert(format!("no-{local_name}"));
            }
        }
        "u" => {
            let value = attribute_value(event, "val").unwrap_or_else(|| "single".to_string());
            if value != "none" {
                tags.insert(format!("u:{value}"));
            }
        }
        "sz" | "szCs" => {
            if let Some(value) = attribute_value(event, "val")
                && let Ok(size) = value.parse::<f64>()
            {
                tags.insert(format!("{local_name}:{:.2}pt", size / 2.0));
            }
        }
        "position" => {
            if let Some(value) = attribute_value(event, "val") {
                tags.insert(format!("position:{value}"));
            }
        }
        "color" => {
            if let Some(value) = attribute_value(event, "val") {
                tags.insert(format!("color:{value}"));
            }
        }
        "highlight" => {
            if let Some(value) = attribute_value(event, "val") {
                tags.insert(format!("highlight:{value}"));
            }
        }
        "jc" => {
            if let Some(value) = attribute_value(event, "val") {
                tags.insert(format!("align:{value}"));
            }
        }
        "rFonts" => {
            for key in ["ascii", "hAnsi", "eastAsia"] {
                if let Some(value) = attribute_value(event, key) {
                    tags.insert(format!("font:{value}"));
                    break;
                }
            }
        }
        _ => {}
    }
}

fn push_break(target: &mut String, local_name: &str) {
    if local_name == "tab" {
        target.push('\t');
    } else {
        target.push('\n');
    }
}

fn attribute_value(event: &BytesStart<'_>, local: &str) -> Option<String> {
    for attribute in event.attributes().with_checks(false).flatten() {
        let key = attribute.key.as_ref();
        if key == local || key.ends_with(&format!(":{local}")) {
            return attribute
                .normalized_value(XmlVersion::Implicit1_0)
                .ok()
                .map(|value| value.into_owned());
        }
    }
    None
}

fn compare_blocks(left: &[ParsedBlock], right: &[ParsedBlock]) -> Vec<BlockChange> {
    if left.len().saturating_mul(right.len()) > 4_000_000 {
        return compare_blocks_positionally(left, right);
    }
    let mut table = vec![vec![0u32; right.len() + 1]; left.len() + 1];
    for left_index in (0..left.len()).rev() {
        for right_index in (0..right.len()).rev() {
            table[left_index][right_index] = if left[left_index].text == right[right_index].text {
                table[left_index + 1][right_index + 1] + 1
            } else {
                table[left_index + 1][right_index].max(table[left_index][right_index + 1])
            };
        }
    }
    let mut left_match = vec![None; left.len()];
    let mut right_match = vec![None; right.len()];
    let mut moved = BTreeSet::new();
    let (mut left_index, mut right_index) = (0, 0);
    while left_index < left.len() && right_index < right.len() {
        if left[left_index].text == right[right_index].text {
            left_match[left_index] = Some(right_index);
            right_match[right_index] = Some(left_index);
            left_index += 1;
            right_index += 1;
        } else if table[left_index + 1][right_index] >= table[left_index][right_index + 1] {
            left_index += 1;
        } else {
            right_index += 1;
        }
    }
    // Exact content that is outside the LCS is a moved block, not an
    // add/remove pair.
    for candidate in 0..left.len() {
        if left_match[candidate].is_some() {
            continue;
        }
        if let Some(target) = (0..right.len()).find(|target| {
            right_match[*target].is_none() && right[*target].text == left[candidate].text
        }) {
            left_match[candidate] = Some(target);
            right_match[target] = Some(candidate);
            moved.insert(candidate);
        }
    }
    // Pair the remaining unmatched blocks in order as changed blocks.
    let unmatched_left: Vec<usize> = (0..left.len())
        .filter(|index| left_match[*index].is_none())
        .collect();
    let unmatched_right: Vec<usize> = (0..right.len())
        .filter(|index| right_match[*index].is_none())
        .collect();
    for (left_block, right_block) in unmatched_left.iter().zip(unmatched_right.iter()) {
        left_match[*left_block] = Some(*right_block);
        right_match[*right_block] = Some(*left_block);
    }

    let mut changes = Vec::new();
    for (right_block, matched) in right_match.iter().enumerate() {
        let Some(left_block) = matched else {
            changes.push(BlockChange {
                path: right[right_block].path.clone(),
                kind: right[right_block].kind.clone(),
                change: ChangeKind::Added,
                left: None,
                right: Some(right[right_block].text.clone()),
                inline: None,
                moved_from: None,
                style: None,
                position: None,
            });
            continue;
        };
        let left_block = *left_block;
        let style = block_style_change(&left[left_block], &right[right_block]);
        let position = block_position_change(&left[left_block], &right[right_block]);
        if moved.contains(&left_block) {
            changes.push(BlockChange {
                path: right[right_block].path.clone(),
                kind: right[right_block].kind.clone(),
                change: ChangeKind::Moved,
                left: Some(left[left_block].text.clone()),
                right: Some(right[right_block].text.clone()),
                inline: None,
                moved_from: Some(left[left_block].path.clone()),
                style,
                position,
            });
        } else if left[left_block].text != right[right_block].text
            || left[left_block].kind != right[right_block].kind
            || left[left_block].style != right[right_block].style
            || position.is_some()
        {
            changes.push(BlockChange {
                path: left[left_block].path.clone(),
                kind: right[right_block].kind.clone(),
                change: ChangeKind::Changed,
                left: Some(left[left_block].text.clone()),
                right: Some(right[right_block].text.clone()),
                inline: Some(text::compare_text(
                    &left[left_block].text,
                    &right[right_block].text,
                    1,
                    40,
                )),
                moved_from: None,
                style,
                position,
            });
        }
    }
    for (left_block, matched) in left_match.iter().enumerate() {
        if matched.is_none() {
            changes.push(BlockChange {
                path: left[left_block].path.clone(),
                kind: left[left_block].kind.clone(),
                change: ChangeKind::Removed,
                left: Some(left[left_block].text.clone()),
                right: None,
                inline: None,
                moved_from: None,
                style: None,
                position: None,
            });
        }
    }
    changes
}

fn block_style_change(left: &ParsedBlock, right: &ParsedBlock) -> Option<FieldChange> {
    (left.style != right.style).then(|| FieldChange {
        path: format!("{}/style", left.path),
        left: left.style.clone(),
        right: right.style.clone(),
    })
}

fn block_position_change(left: &ParsedBlock, right: &ParsedBlock) -> Option<FieldChange> {
    (left.position != right.position).then(|| FieldChange {
        path: format!("{}/position", left.path),
        left: left.position.clone(),
        right: right.position.clone(),
    })
}

fn compare_blocks_positionally(left: &[ParsedBlock], right: &[ParsedBlock]) -> Vec<BlockChange> {
    let mut changes = Vec::new();
    for index in 0..left.len().max(right.len()) {
        match (left.get(index), right.get(index)) {
            (Some(left_block), Some(right_block))
                if left_block.text != right_block.text
                    || left_block.kind != right_block.kind
                    || left_block.style != right_block.style
                    || left_block.position != right_block.position =>
            {
                changes.push(BlockChange {
                    path: left_block.path.clone(),
                    kind: right_block.kind.clone(),
                    change: ChangeKind::Changed,
                    left: Some(left_block.text.clone()),
                    right: Some(right_block.text.clone()),
                    inline: Some(text::compare_text(
                        &left_block.text,
                        &right_block.text,
                        1,
                        40,
                    )),
                    moved_from: None,
                    style: block_style_change(left_block, right_block),
                    position: block_position_change(left_block, right_block),
                });
            }
            (Some(left_block), None) => changes.push(BlockChange {
                path: left_block.path.clone(),
                kind: left_block.kind.clone(),
                change: ChangeKind::Removed,
                left: Some(left_block.text.clone()),
                right: None,
                inline: None,
                moved_from: None,
                style: None,
                position: None,
            }),
            (None, Some(right_block)) => changes.push(BlockChange {
                path: right_block.path.clone(),
                kind: right_block.kind.clone(),
                change: ChangeKind::Added,
                left: None,
                right: Some(right_block.text.clone()),
                inline: None,
                moved_from: None,
                style: None,
                position: None,
            }),
            _ => {}
        }
    }
    changes
}

fn compare_docx_tables(
    left: &[ParsedTableRows],
    right: &[ParsedTableRows],
    max_rows: usize,
) -> Vec<DocumentTableDiff> {
    let mut tables = Vec::new();
    for index in 0..left.len().min(right.len()) {
        let left_rows = rows_to_csv(&left[index].rows);
        let right_rows = rows_to_csv(&right[index].rows);
        if let Ok(table) =
            table::compare_tables(&left_rows, &right_rows, "docx-table", None, max_rows)
        {
            tables.push(DocumentTableDiff {
                path: left[index].path.clone(),
                table,
            });
        }
    }
    tables
}

fn rows_to_csv(rows: &[Vec<String>]) -> Vec<u8> {
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(Vec::new());
    for row in rows {
        let _ = writer.write_record(row);
    }
    writer.into_inner().unwrap_or_default()
}

fn media_hashes(package: &Package<'_>, prefix: &str) -> MediaHashes {
    package
        .names
        .iter()
        .filter(|name| name.starts_with(prefix))
        .filter_map(|name| {
            let data = package.read(name).ok()?;
            Some((name.clone(), (hex_digest(&data), data.len() as u64)))
        })
        .collect()
}

fn compare_media_maps(
    left_media: &MediaHashes,
    right_media: &MediaHashes,
    kind: &str,
) -> Vec<MediaChange> {
    let names: BTreeSet<&String> = left_media.keys().chain(right_media.keys()).collect();
    let mut changes = Vec::new();
    for name in names {
        let left_value = left_media.get(name);
        let right_value = right_media.get(name);
        if left_value.map(|value| &value.0) == right_value.map(|value| &value.0) {
            continue;
        }
        changes.push(MediaChange {
            path: name.clone(),
            kind: kind.to_string(),
            left_sha256: left_value.map(|value| value.0.clone()),
            right_sha256: right_value.map(|value| value.0.clone()),
            left_size: left_value.map(|value| value.1),
            right_size: right_value.map(|value| value.1),
        });
    }
    changes
}

fn hex_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[derive(Debug, Clone, Default)]
struct ParsedShape {
    name: String,
    kind: String,
    text: String,
    geometry: Option<String>,
    font_metrics: Option<String>,
    table: Option<Vec<Vec<String>>>,
    media: Option<String>,
    media_sha256: Option<String>,
    media_size: Option<u64>,
}

#[derive(Debug, Clone)]
struct ParsedSlide {
    index: usize,
    hidden: bool,
    title: Option<String>,
    shapes: Vec<ParsedShape>,
    notes: Option<String>,
    layout: Option<String>,
}

/// Compares two PPTX packages by slide, then by shape inside each slide.
pub fn compare_pptx_documents(
    left: &[u8],
    right: &[u8],
    max_slides: usize,
    max_table_rows: usize,
) -> std::result::Result<PresentationDiff, String> {
    let (left_result, right_result) = rayon::join(
        || {
            let package = Package::new(left)?;
            parse_pptx_slides(&package, max_slides)
        },
        || {
            let package = Package::new(right)?;
            parse_pptx_slides(&package, max_slides)
        },
    );
    let (left_slides, left_truncated) = left_result?;
    let (right_slides, right_truncated) = right_result?;
    let left_keys: Vec<String> = left_slides.iter().map(slide_fingerprint).collect();
    let right_keys: Vec<String> = right_slides.iter().map(slide_fingerprint).collect();
    let (left_match, right_match, moved) = align_keys(&left_keys, &right_keys);

    let mut slides = Vec::new();
    let mut slides_added = 0usize;
    let mut slides_removed = 0usize;
    let mut slides_moved = 0usize;
    for (right_index, matched) in right_match.iter().enumerate() {
        let right_slide = &right_slides[right_index];
        match matched {
            Some(left_index) => {
                let left_slide = &left_slides[*left_index];
                let shapes = compare_shapes(
                    &left_slide.shapes,
                    &right_slide.shapes,
                    right_index + 1,
                    max_table_rows,
                );
                let notes = match (&left_slide.notes, &right_slide.notes) {
                    (Some(left_notes), Some(right_notes)) if left_notes != right_notes => {
                        Some(text::compare_text(left_notes, right_notes, 1, 40))
                    }
                    (None, Some(right_notes)) if !right_notes.is_empty() => {
                        Some(text::compare_text("", right_notes, 1, 40))
                    }
                    (Some(left_notes), None) if !left_notes.is_empty() => {
                        Some(text::compare_text(left_notes, "", 1, 40))
                    }
                    _ => None,
                };
                let layout = match (&left_slide.layout, &right_slide.layout) {
                    (left_layout, right_layout) if left_layout != right_layout => {
                        Some(FieldChange {
                            path: format!("slide[{}]/layout", right_index + 1),
                            left: left_layout.clone(),
                            right: right_layout.clone(),
                        })
                    }
                    _ => None,
                };
                let visibility = (left_slide.hidden != right_slide.hidden).then(|| FieldChange {
                    path: format!("slide[{}]/hidden", right_index + 1),
                    left: Some(left_slide.hidden.to_string()),
                    right: Some(right_slide.hidden.to_string()),
                });
                let is_moved = moved.contains(left_index);
                if !is_moved
                    && shapes.is_empty()
                    && notes.is_none()
                    && layout.is_none()
                    && visibility.is_none()
                    && left_slide.title == right_slide.title
                {
                    continue;
                }
                if is_moved {
                    slides_moved += 1;
                }
                slides.push(SlideChange {
                    left_index: Some(left_slide.index + 1),
                    right_index: Some(right_slide.index + 1),
                    title: right_slide.title.clone().or(left_slide.title.clone()),
                    change: if is_moved {
                        ChangeKind::Moved
                    } else {
                        ChangeKind::Changed
                    },
                    moved_from: is_moved.then_some(left_slide.index + 1),
                    layout,
                    visibility,
                    shapes,
                    notes,
                });
            }
            None => {
                slides_added += 1;
                slides.push(SlideChange {
                    left_index: None,
                    right_index: Some(right_slide.index + 1),
                    title: right_slide.title.clone(),
                    change: ChangeKind::Added,
                    moved_from: None,
                    layout: None,
                    visibility: None,
                    shapes: compare_shapes(
                        &[],
                        &right_slide.shapes,
                        right_index + 1,
                        max_table_rows,
                    ),
                    notes: None,
                });
            }
        }
    }
    for (left_index, matched) in left_match.iter().enumerate() {
        if matched.is_none() {
            slides_removed += 1;
            let left_slide = &left_slides[left_index];
            slides.push(SlideChange {
                left_index: Some(left_slide.index + 1),
                right_index: None,
                title: left_slide.title.clone(),
                change: ChangeKind::Removed,
                moved_from: None,
                layout: None,
                visibility: None,
                shapes: Vec::new(),
                notes: None,
            });
        }
    }

    Ok(PresentationDiff {
        format: "pptx".to_string(),
        left_slides: left_slides.len(),
        right_slides: right_slides.len(),
        slides_added,
        slides_removed,
        slides_moved,
        slides,
        truncated: left_truncated || right_truncated,
    })
}

fn parse_pptx_slides(
    package: &Package<'_>,
    max_slides: usize,
) -> std::result::Result<(Vec<ParsedSlide>, bool), String> {
    let order = slide_part_order(package)?;
    let truncated = order.len() > max_slides;
    let mut slides = Vec::new();
    for (index, (part, hidden)) in order.iter().take(max_slides).enumerate() {
        let xml = package.read_text(part)?;
        let mut shapes = parse_slide_shapes(&xml)?;
        let rels = slide_relationships(package, part)?;
        let base = part
            .rsplit_once('/')
            .map(|(dir, _)| dir)
            .unwrap_or("ppt/slides");
        for shape in &mut shapes {
            if let Some(media) = &shape.media
                && let Some(target) = rels.get(media)
            {
                let target = resolve_part_path(base, target);
                if let Ok(data) = package.read(&target) {
                    shape.media_sha256 = Some(hex_digest(&data));
                    shape.media_size = Some(data.len() as u64);
                }
                shape.media = Some(target);
            }
        }
        let notes = slide_notes_part(package, part)?
            .and_then(|notes_part| package.read_text(&notes_part).ok())
            .and_then(|xml| extract_slide_text(&xml).ok())
            .filter(|notes| !notes.trim().is_empty());
        let layout = slide_layout_part(package, part)?;
        let title = shapes
            .iter()
            .find(|shape| shape.kind == "title" && !shape.text.trim().is_empty())
            .map(|shape| first_line(&shape.text))
            .or_else(|| {
                shapes
                    .iter()
                    .find(|shape| !shape.text.trim().is_empty())
                    .map(|shape| first_line(&shape.text))
            });
        slides.push(ParsedSlide {
            index,
            hidden: *hidden,
            title,
            shapes,
            notes,
            layout,
        });
    }
    Ok((slides, truncated))
}

fn slide_relationships(
    package: &Package<'_>,
    slide: &str,
) -> std::result::Result<BTreeMap<String, String>, String> {
    let file = slide.rsplit('/').next().unwrap_or(slide);
    let rels_name = format!("ppt/slides/_rels/{file}.rels");
    let xml = match package.read_text(&rels_name) {
        Ok(xml) => xml,
        Err(_) => return Ok(BTreeMap::new()),
    };
    let mut map = BTreeMap::new();
    for (id, _, target) in relationships(&xml)? {
        map.insert(id, target);
    }
    Ok(map)
}

fn slide_layout_part(
    package: &Package<'_>,
    slide: &str,
) -> std::result::Result<Option<String>, String> {
    let file = slide.rsplit('/').next().unwrap_or(slide);
    let rels_name = format!("ppt/slides/_rels/{file}.rels");
    let Ok(xml) = package.read_text(&rels_name) else {
        return Ok(None);
    };
    for (_, kind, target) in relationships(&xml)? {
        if kind.ends_with("/slideLayout") {
            let target = resolve_part_path("ppt/slides", &target);
            if package.contains(&target) {
                return Ok(Some(target));
            }
        }
    }
    Ok(None)
}

fn parse_slide_shapes(xml: &str) -> std::result::Result<Vec<ParsedShape>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut shapes = Vec::new();
    let mut current: Option<ParsedShape> = None;
    let mut in_text = false;
    let mut in_table = false;
    let mut in_cell = false;
    let mut in_run_props = false;
    let mut font_tags: BTreeSet<String> = BTreeSet::new();
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cell = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => match event.local_name().as_ref() {
                "sp" | "pic" | "graphicFrame" if current.is_none() => {
                    current = Some(ParsedShape {
                        kind: if event.local_name().as_ref() == "pic" {
                            "picture".to_string()
                        } else {
                            "shape".to_string()
                        },
                        ..ParsedShape::default()
                    });
                }
                "cNvPr" => {
                    if let Some(shape) = &mut current
                        && let Some(name) = attribute_value(&event, "name")
                    {
                        shape.name = name;
                    }
                }
                "ph" => {
                    if let Some(shape) = &mut current
                        && let Some(kind) = attribute_value(&event, "type")
                    {
                        shape.kind = if matches!(kind.as_str(), "title" | "ctrTitle") {
                            "title".to_string()
                        } else {
                            kind
                        };
                    }
                }
                "off" => {
                    if let Some(shape) = &mut current {
                        let x = attribute_value(&event, "x").unwrap_or_else(|| "?".to_string());
                        let y = attribute_value(&event, "y").unwrap_or_else(|| "?".to_string());
                        shape.geometry = Some(format!("pos({x},{y})"));
                    }
                }
                "ext" => {
                    if let Some(shape) = &mut current {
                        let width =
                            attribute_value(&event, "cx").unwrap_or_else(|| "?".to_string());
                        let height =
                            attribute_value(&event, "cy").unwrap_or_else(|| "?".to_string());
                        let prefix = shape.geometry.take().unwrap_or_default();
                        shape.geometry = Some(format!("{prefix} size({width},{height})"));
                    }
                }
                "rPr" | "defRPr" => {
                    in_run_props = true;
                    apply_ppt_run_attributes(&event, &mut font_tags);
                }
                "latin" if in_run_props => {
                    if let Some(typeface) = attribute_value(&event, "typeface") {
                        font_tags.insert(format!("font:{typeface}"));
                    }
                }
                "srgbClr" if in_run_props => {
                    if let Some(value) = attribute_value(&event, "val") {
                        font_tags.insert(format!("color:{value}"));
                    }
                }
                "tbl" if current.is_some() => {
                    in_table = true;
                    table_rows.clear();
                }
                "tr" if in_table => row.clear(),
                "tc" if in_table => {
                    cell.clear();
                    in_cell = true;
                }
                "t" => in_text = true,
                "blip" => {
                    if let Some(shape) = &mut current
                        && let Some(embed) = attribute_value(&event, "embed")
                    {
                        shape.media = Some(embed);
                        shape.kind = "picture".to_string();
                    }
                }
                "chart" => {
                    if let Some(shape) = &mut current
                        && let Some(id) = attribute_value(&event, "id")
                    {
                        shape.media = Some(id);
                        shape.kind = "chart".to_string();
                    }
                }
                "br" if current.is_some() => {
                    if let Some(shape) = &mut current {
                        if in_table && in_cell {
                            cell.push('\n');
                        } else {
                            shape.text.push('\n');
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::Empty(event)) => match event.local_name().as_ref() {
                "rPr" | "defRPr" => {
                    apply_ppt_run_attributes(&event, &mut font_tags);
                }
                "latin" if in_run_props => {
                    if let Some(typeface) = attribute_value(&event, "typeface") {
                        font_tags.insert(format!("font:{typeface}"));
                    }
                }
                "srgbClr" if in_run_props => {
                    if let Some(value) = attribute_value(&event, "val") {
                        font_tags.insert(format!("color:{value}"));
                    }
                }
                "cNvPr" => {
                    if let Some(shape) = &mut current
                        && let Some(name) = attribute_value(&event, "name")
                    {
                        shape.name = name;
                    }
                }
                "ph" => {
                    if let Some(shape) = &mut current
                        && let Some(kind) = attribute_value(&event, "type")
                    {
                        shape.kind = if matches!(kind.as_str(), "title" | "ctrTitle") {
                            "title".to_string()
                        } else {
                            kind
                        };
                    }
                }
                "off" => {
                    if let Some(shape) = &mut current {
                        let x = attribute_value(&event, "x").unwrap_or_else(|| "?".to_string());
                        let y = attribute_value(&event, "y").unwrap_or_else(|| "?".to_string());
                        shape.geometry = Some(format!("pos({x},{y})"));
                    }
                }
                "ext" => {
                    if let Some(shape) = &mut current {
                        let width =
                            attribute_value(&event, "cx").unwrap_or_else(|| "?".to_string());
                        let height =
                            attribute_value(&event, "cy").unwrap_or_else(|| "?".to_string());
                        let prefix = shape.geometry.take().unwrap_or_default();
                        shape.geometry = Some(format!("{prefix} size({width},{height})"));
                    }
                }
                "br" => {
                    if let Some(shape) = &mut current {
                        shape.text.push('\n');
                    }
                }
                _ => {}
            },
            Ok(Event::Text(event)) if in_text => {
                let value = event.xml_content(XmlVersion::Implicit1_0);
                if in_table {
                    if in_cell {
                        cell.push_str(&value);
                    }
                } else if let Some(shape) = &mut current {
                    shape.text.push_str(&value);
                }
            }
            Ok(Event::CData(event)) if in_text => {
                if in_table {
                    if in_cell {
                        cell.push_str(event.as_ref());
                    }
                } else if let Some(shape) = &mut current {
                    shape.text.push_str(event.as_ref());
                }
            }
            Ok(Event::End(event)) => match event.local_name().as_ref() {
                "rPr" | "defRPr" => {
                    in_run_props = false;
                    if let Some(shape) = &mut current
                        && !font_tags.is_empty()
                    {
                        let mut tags: BTreeSet<String> = shape
                            .font_metrics
                            .take()
                            .unwrap_or_default()
                            .split(',')
                            .filter(|tag| !tag.is_empty())
                            .map(str::to_string)
                            .collect();
                        tags.extend(font_tags.iter().cloned());
                        shape.font_metrics = Some(tags.into_iter().collect::<Vec<_>>().join(","));
                        font_tags.clear();
                    }
                }
                "t" => in_text = false,
                "tc" if in_table => {
                    row.push(cell.trim().to_string());
                    in_cell = false;
                }
                "tr" if in_table => {
                    if !row.is_empty() {
                        table_rows.push(std::mem::take(&mut row));
                    }
                }
                "tbl" if in_table => {
                    in_table = false;
                    if let Some(shape) = &mut current {
                        shape.table = Some(std::mem::take(&mut table_rows));
                        shape.kind = "table".to_string();
                    }
                }
                "p" => {
                    if let Some(shape) = &mut current {
                        if in_table && in_cell {
                            if !cell.ends_with('\n') {
                                cell.push('\n');
                            }
                        } else if !shape.text.ends_with('\n') {
                            shape.text.push('\n');
                        }
                    }
                }
                "sp" | "pic" | "graphicFrame" => {
                    if let Some(mut shape) = current.take() {
                        shape.text = shape.text.trim().to_string();
                        if shape.name.is_empty() {
                            shape.name = format!("shape {}", shapes.len() + 1);
                        }
                        if !shape.text.is_empty()
                            || shape.table.as_ref().is_some_and(|rows| !rows.is_empty())
                            || shape.media.is_some()
                        {
                            shapes.push(shape);
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(shapes)
}

fn slide_fingerprint(slide: &ParsedSlide) -> String {
    let mut value = String::new();
    if let Some(title) = &slide.title {
        value.push_str(title);
        value.push('\n');
    }
    for shape in &slide.shapes {
        value.push_str(&shape.name);
        value.push(':');
        value.push_str(&shape.text);
        value.push('\n');
        if let Some(rows) = &shape.table {
            for row in rows {
                value.push_str(&row.join("\t"));
                value.push('\n');
            }
        }
        if let Some(media) = &shape.media {
            value.push_str(media);
            value.push('\n');
        }
    }
    normalize_text(value)
}

fn align_keys(
    left: &[String],
    right: &[String],
) -> (Vec<Option<usize>>, Vec<Option<usize>>, BTreeSet<usize>) {
    if left.len().saturating_mul(right.len()) > 4_000_000 {
        let mut left_match = vec![None; left.len()];
        let mut right_match = vec![None; right.len()];
        for index in 0..left.len().min(right.len()) {
            left_match[index] = Some(index);
            right_match[index] = Some(index);
        }
        return (left_match, right_match, BTreeSet::new());
    }
    let mut table = vec![vec![0u32; right.len() + 1]; left.len() + 1];
    for left_index in (0..left.len()).rev() {
        for right_index in (0..right.len()).rev() {
            table[left_index][right_index] = if left[left_index] == right[right_index] {
                table[left_index + 1][right_index + 1] + 1
            } else {
                table[left_index + 1][right_index].max(table[left_index][right_index + 1])
            };
        }
    }
    let mut left_match = vec![None; left.len()];
    let mut right_match = vec![None; right.len()];
    let mut moved = BTreeSet::new();
    let (mut left_index, mut right_index) = (0, 0);
    while left_index < left.len() && right_index < right.len() {
        if left[left_index] == right[right_index] {
            left_match[left_index] = Some(right_index);
            right_match[right_index] = Some(left_index);
            left_index += 1;
            right_index += 1;
        } else if table[left_index + 1][right_index] >= table[left_index][right_index + 1] {
            left_index += 1;
        } else {
            right_index += 1;
        }
    }
    for candidate in 0..left.len() {
        if left_match[candidate].is_some() {
            continue;
        }
        if let Some(target) = (0..right.len())
            .find(|target| right_match[*target].is_none() && right[*target] == left[candidate])
        {
            left_match[candidate] = Some(target);
            right_match[target] = Some(candidate);
            moved.insert(candidate);
        }
    }
    let unmatched_left: Vec<usize> = (0..left.len())
        .filter(|index| left_match[*index].is_none())
        .collect();
    let unmatched_right: Vec<usize> = (0..right.len())
        .filter(|index| right_match[*index].is_none())
        .collect();
    for (left_block, right_block) in unmatched_left.iter().zip(unmatched_right.iter()) {
        left_match[*left_block] = Some(*right_block);
        right_match[*right_block] = Some(*left_block);
    }
    (left_match, right_match, moved)
}

fn compare_shapes(
    left: &[ParsedShape],
    right: &[ParsedShape],
    slide_index: usize,
    max_table_rows: usize,
) -> Vec<ShapeChange> {
    let left_by_name: BTreeMap<&str, &ParsedShape> = left
        .iter()
        .map(|shape| (shape.name.as_str(), shape))
        .collect();
    let right_by_name: BTreeMap<&str, &ParsedShape> = right
        .iter()
        .map(|shape| (shape.name.as_str(), shape))
        .collect();
    let names: BTreeSet<&str> = left_by_name
        .keys()
        .chain(right_by_name.keys())
        .copied()
        .collect();
    let mut changes = Vec::new();
    for name in names {
        let left_shape = left_by_name.get(name);
        let right_shape = right_by_name.get(name);
        let path = format!("slide[{slide_index}]/shape[{name}]");
        match (left_shape, right_shape) {
            (Some(left_shape), Some(right_shape)) => {
                let text_diff = (left_shape.text != right_shape.text)
                    .then(|| text::compare_text(&left_shape.text, &right_shape.text, 1, 40));
                let table_diff = match (&left_shape.table, &right_shape.table) {
                    (Some(left_rows), Some(right_rows)) => table::compare_tables(
                        &rows_to_csv(left_rows),
                        &rows_to_csv(right_rows),
                        "pptx-table",
                        None,
                        max_table_rows,
                    )
                    .ok(),
                    _ => None,
                };
                let media = match (
                    &left_shape.media_sha256,
                    &right_shape.media_sha256,
                    &left_shape.media,
                    &right_shape.media,
                ) {
                    (left_hash, right_hash, _, _)
                        if left_hash.is_some() || right_hash.is_some() =>
                    {
                        if left_hash == right_hash {
                            None
                        } else {
                            Some(MediaChange {
                                path: right_shape
                                    .media
                                    .clone()
                                    .or_else(|| left_shape.media.clone())
                                    .unwrap_or_else(|| name.to_string()),
                                kind: right_shape.kind.clone(),
                                left_sha256: left_hash.clone(),
                                right_sha256: right_hash.clone(),
                                left_size: left_shape.media_size,
                                right_size: right_shape.media_size,
                            })
                        }
                    }
                    _ => None,
                };
                let geometry = (left_shape.geometry != right_shape.geometry).then(|| FieldChange {
                    path: format!("{path}/geometry"),
                    left: left_shape.geometry.clone(),
                    right: right_shape.geometry.clone(),
                });
                let font =
                    (left_shape.font_metrics != right_shape.font_metrics).then(|| FieldChange {
                        path: format!("{path}/font"),
                        left: left_shape.font_metrics.clone(),
                        right: right_shape.font_metrics.clone(),
                    });
                if text_diff.is_some()
                    || table_diff.is_some()
                    || media.is_some()
                    || geometry.is_some()
                    || font.is_some()
                    || left_shape.kind != right_shape.kind
                {
                    changes.push(ShapeChange {
                        path,
                        name: name.to_string(),
                        kind: right_shape.kind.clone(),
                        change: ChangeKind::Changed,
                        geometry,
                        font,
                        text: text_diff,
                        table: table_diff,
                        media,
                    });
                }
            }
            (Some(left_shape), None) => changes.push(ShapeChange {
                path,
                name: name.to_string(),
                kind: left_shape.kind.clone(),
                change: ChangeKind::Removed,
                geometry: None,
                font: None,
                text: None,
                table: None,
                media: None,
            }),
            (None, Some(right_shape)) => changes.push(ShapeChange {
                path,
                name: name.to_string(),
                kind: right_shape.kind.clone(),
                change: ChangeKind::Added,
                geometry: None,
                font: None,
                text: None,
                table: None,
                media: None,
            }),
            (None, None) => {}
        }
    }
    changes
}

fn is_word_text_part(name: &str) -> bool {
    if name == "word/document.xml"
        || name == "word/footnotes.xml"
        || name == "word/endnotes.xml"
        || name == "word/comments.xml"
    {
        return true;
    }
    (name.starts_with("word/header") || name.starts_with("word/footer")) && name.ends_with(".xml")
}

fn numbered_parts(names: &[String], prefix: &str, suffix: &str) -> Vec<(u32, String)> {
    names
        .iter()
        .filter_map(|name| {
            let number = name
                .strip_prefix(prefix)?
                .strip_suffix(suffix)?
                .parse::<u32>()
                .ok()?;
            Some((number, name.clone()))
        })
        .collect()
}

fn extract_word_xml(xml: &str) -> std::result::Result<String, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut out = String::new();
    let mut in_text = false;
    let mut in_table = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => match event.local_name().as_ref() {
                "t" => in_text = true,
                "tbl" if !in_table => {
                    in_table = true;
                    out.push_str("\n# Table\n");
                }
                _ => {}
            },
            Ok(Event::End(event)) => match event.local_name().as_ref() {
                "t" => in_text = false,
                "p" => out.push('\n'),
                "tc" => out.push('\t'),
                "tr" => out.push('\n'),
                "tbl" => {
                    in_table = false;
                    out.push('\n');
                }
                _ => {}
            },
            Ok(Event::Empty(event)) => match event.local_name().as_ref() {
                "br" => out.push('\n'),
                "tab" => out.push('\t'),
                _ => {}
            },
            Ok(Event::Text(event)) if in_text => {
                out.push_str(&event.xml_content(quick_xml::XmlVersion::Implicit1_0));
            }
            Ok(Event::CData(event)) if in_text => {
                out.push_str(event.as_ref());
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(error.to_string()),
        }
    }

    Ok(normalize_text(out))
}

fn extract_slide_text(xml: &str) -> std::result::Result<String, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut out = String::new();
    let mut in_text = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                if event.local_name().as_ref() == "t" {
                    in_text = true;
                }
            }
            Ok(Event::End(event)) => match event.local_name().as_ref() {
                "t" => in_text = false,
                "p" => out.push('\n'),
                _ => {}
            },
            Ok(Event::Text(event)) if in_text => {
                out.push_str(&event.xml_content(quick_xml::XmlVersion::Implicit1_0));
            }
            Ok(Event::CData(event)) if in_text => {
                out.push_str(event.as_ref());
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(error.to_string()),
        }
    }

    Ok(out.trim().to_string())
}

fn first_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .to_string()
}

fn normalize_text(text: String) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank_lines = 0;
    for line in text.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            blank_lines += 1;
            if blank_lines > 1 {
                continue;
            }
        } else {
            blank_lines = 0;
        }
        out.push_str(trimmed);
        out.push('\n');
    }
    out.trim_end().to_string()
}

#[cfg(test)]
#[path = "../../tests/unit/diff/documents.rs"]
mod tests;
