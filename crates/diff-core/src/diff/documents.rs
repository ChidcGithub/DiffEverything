use quick_xml::Reader;
use quick_xml::events::Event;
use std::io::Read;
use zip::ZipArchive;

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
    let names = list_zip_names(bytes)?;
    let mut pages: Vec<String> = names
        .into_iter()
        .filter(|name| name.contains("/Pages/") && name.ends_with(".xml"))
        .collect();
    pages.sort();
    if pages.is_empty() {
        return Err("no OFD page XML found".to_string());
    }

    let mut out = String::new();
    for (index, name) in pages.iter().enumerate() {
        let xml = read_zip_entry(bytes, name)?;
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
    let names = list_zip_names(bytes)?;
    let mut part_names: Vec<String> = names
        .into_iter()
        .filter(|name| is_word_text_part(name))
        .collect();
    part_names.sort();
    if !part_names.iter().any(|name| name == "word/document.xml") {
        return Err("DOCX has no word/document.xml".to_string());
    }

    let mut out = String::new();
    for name in part_names {
        let xml = read_zip_entry(bytes, &name)?;
        let text = extract_word_xml(&xml)?;
        if text.trim().is_empty() {
            continue;
        }
        if name != "word/document.xml" {
            let label = name.rsplit('/').next().unwrap_or(&name);
            out.push_str(&format!("# {label}\n"));
        }
        out.push_str(&text);
        out.push_str("\n\n");
    }
    Ok(normalize_text(out))
}

pub fn extract_pptx_text(bytes: &[u8]) -> std::result::Result<String, String> {
    let names = list_zip_names(bytes)?;
    let mut slides = numbered_parts(&names, "ppt/slides/slide", ".xml");
    let notes = numbered_parts(&names, "ppt/notesSlides/notesSlide", ".xml");
    slides.sort_by_key(|(number, _)| *number);

    if slides.is_empty() {
        return Err("no slides found in PPTX".to_string());
    }

    let mut out = String::new();
    for (number, name) in slides {
        let xml = read_zip_entry(bytes, &name)?;
        let text = extract_slide_text(&xml)?;
        out.push_str(&format!("# Slide {number}\n{text}\n\n"));
    }
    for (number, name) in notes {
        let xml = read_zip_entry(bytes, &name)?;
        let text = extract_slide_text(&xml)?;
        if !text.trim().is_empty() {
            out.push_str(&format!("# Notes {number}\n{text}\n\n"));
        }
    }
    Ok(normalize_text(out))
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
                "tc" => out.push('\t'),
                "tr" => out.push('\n'),
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

fn read_zip_entry(bytes: &[u8], name: &str) -> std::result::Result<String, String> {
    let mut archive =
        ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|error| error.to_string())?;
    let mut entry = archive.by_name(name).map_err(|error| error.to_string())?;
    let mut xml = String::new();
    entry
        .read_to_string(&mut xml)
        .map_err(|error| error.to_string())?;
    Ok(xml)
}

fn list_zip_names(bytes: &[u8]) -> std::result::Result<Vec<String>, String> {
    let mut archive =
        ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|error| error.to_string())?;
    let mut names = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|error| error.to_string())?;
        names.push(entry.name().to_string());
    }
    Ok(names)
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
