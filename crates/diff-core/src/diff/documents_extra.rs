use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};
use scraper::Html;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read as _};
use zip::ZipArchive;

const MAX_ENTRY_BYTES: usize = 32 * 1024 * 1024;

pub fn parse_odt(bytes: &[u8]) -> Result<Value, String> {
    parse_odf(bytes, "odt")
}

pub fn parse_odp(bytes: &[u8]) -> Result<Value, String> {
    parse_odf(bytes, "odp")
}

pub fn extract_odt_text(bytes: &[u8]) -> Result<String, String> {
    extract_odf_text(bytes, "odt")
}

pub fn extract_odp_text(bytes: &[u8]) -> Result<String, String> {
    extract_odf_text(bytes, "odp")
}

fn extract_odf_text(bytes: &[u8], format: &str) -> Result<String, String> {
    let value = parse_odf(bytes, format)?;
    Ok(value
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string())
}

fn parse_odf(bytes: &[u8], format: &str) -> Result<Value, String> {
    let files = unzip(bytes)?;
    let content = files
        .get("content.xml")
        .ok_or_else(|| format!("{format}: missing content.xml"))?;
    let (text, paragraphs, slides) = xml_text_counts(content)?;
    let metadata = files
        .get("meta.xml")
        .map(|xml| metadata_fields(xml))
        .transpose()?
        .unwrap_or_default();
    let entries = entry_inventory(&files);
    Ok(json!({
        "format": format,
        "mimetype": text_entry(&files, "mimetype"),
        "metadata": metadata,
        "text": text,
        "paragraphs": paragraphs,
        "slides": slides,
        "entries": entries,
    }))
}

pub fn parse_epub(bytes: &[u8]) -> Result<Value, String> {
    let files = unzip(bytes)?;
    let container = files
        .get("META-INF/container.xml")
        .ok_or_else(|| "epub: missing META-INF/container.xml".to_string())?;
    let rootfile = epub_rootfile(container)?;
    let opf = files
        .get(&rootfile)
        .ok_or_else(|| format!("epub: missing OPF {rootfile}"))?;
    let (metadata, manifest, spine) = epub_package(opf)?;
    let base = rootfile
        .rsplit_once('/')
        .map(|(directory, _)| directory)
        .unwrap_or("");
    let mut chapters = Vec::new();
    for idref in &spine {
        let Some(href) = manifest.get(idref) else {
            continue;
        };
        let path = resolve_path(base, href);
        let Some(data) = files.get(&path) else {
            continue;
        };
        chapters.push(json!({
            "href": path,
            "text": html_text(data),
        }));
    }
    let entries = entry_inventory(&files);
    Ok(json!({
        "format": "epub",
        "mimetype": text_entry(&files, "mimetype"),
        "rootfile": rootfile,
        "metadata": metadata,
        "spine": spine,
        "chapters": chapters,
        "entries": entries,
    }))
}

/// Parse RTF into paragraphs plus the set of control words used.
pub fn parse_rtf(bytes: &[u8]) -> Result<Value, String> {
    let (text, control_words) = rtf_text(bytes)?;
    let paragraphs: Vec<Value> = text
        .split('\n')
        .map(str::trim)
        .filter(|paragraph| !paragraph.is_empty())
        .map(|paragraph| Value::String(paragraph.to_string()))
        .collect();
    Ok(json!({
        "format": "rtf",
        "paragraphs": paragraphs,
        "control_words": control_words.into_iter().collect::<Vec<_>>(),
        "characters": text.chars().count(),
    }))
}

/// Parse an RFC 5322 / MIME message into headers, text bodies, attachments and
/// a recursive part tree.
pub fn parse_eml(bytes: &[u8]) -> Result<Value, String> {
    let raw = decode_text(bytes, None);
    let root = parse_mime_entity(&raw, 0)?;
    let mut texts = Vec::new();
    let mut html = Vec::new();
    let mut attachments = Vec::new();
    collect_mail_parts(&root, &mut texts, &mut html, &mut attachments);
    let headers = root
        .get("headers")
        .cloned()
        .unwrap_or_else(|| Value::Object(Map::new()));
    Ok(json!({
        "format": "eml",
        "headers": headers,
        "text": texts.join("\n\n"),
        "html": html.join("\n\n"),
        "attachments": attachments,
        "parts": root,
    }))
}

pub(crate) fn unzip(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).map_err(|error| format!("zip: {error}"))?;
    let mut files = BTreeMap::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("zip entry: {error}"))?;
        if entry.size() > MAX_ENTRY_BYTES as u64 {
            continue;
        }
        let name = entry.name().to_string();
        let mut data = Vec::new();
        entry
            .read_to_end(&mut data)
            .map_err(|error| format!("zip read {name}: {error}"))?;
        files.insert(name, data);
    }
    Ok(files)
}

fn text_entry(files: &BTreeMap<String, Vec<u8>>, name: &str) -> Value {
    files
        .get(name)
        .map(|data| Value::String(String::from_utf8_lossy(data).trim().to_string()))
        .unwrap_or(Value::Null)
}

pub(crate) fn entry_inventory(files: &BTreeMap<String, Vec<u8>>) -> Value {
    Value::Array(
        files
            .iter()
            .map(|(name, data)| {
                json!({
                    "name": name,
                    "size": data.len(),
                    "sha256": hex(&Sha256::digest(data)),
                })
            })
            .collect(),
    )
}

fn xml_text_counts(xml: &[u8]) -> Result<(String, usize, usize), String> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut text = String::new();
    let mut paragraphs = 0usize;
    let mut slides = 0usize;
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) | Ok(Event::Empty(event)) => {
                let name = event.local_name();
                match name.as_ref() {
                    "p" | "h" => {
                        paragraphs += 1;
                        ensure_newline(&mut text);
                    }
                    "page" => {
                        slides += 1;
                        ensure_newline(&mut text);
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(event)) => {
                text.push_str(&event.xml_content(XmlVersion::Implicit1_0));
            }
            Ok(Event::CData(event)) => {
                text.push_str(event.as_ref());
            }
            Ok(Event::End(event)) => {
                if matches!(event.local_name().as_ref(), "p" | "h" | "page") {
                    ensure_newline(&mut text);
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok((text.trim().to_string(), paragraphs, slides))
}

pub(crate) fn metadata_fields(xml: &[u8]) -> Result<Value, String> {
    let keys: BTreeSet<&str> = [
        "title",
        "subject",
        "creator",
        "initial-creator",
        "description",
        "generator",
        "creation-date",
        "modification-date",
        "language",
        "keywords",
        "editing-cycles",
        "editing-duration",
    ]
    .into_iter()
    .collect();
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut map = Map::new();
    let mut current: Option<String> = None;
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                let name = event.local_name().as_ref().to_string();
                current = keys.contains(name.as_str()).then_some(name);
            }
            Ok(Event::Text(event)) => {
                if let Some(name) = &current {
                    let text = event.xml_content(XmlVersion::Implicit1_0);
                    map.entry(name.clone())
                        .or_insert_with(|| Value::String(String::new()));
                    if let Some(Value::String(existing)) = map.get_mut(name) {
                        existing.push_str(&text);
                    }
                }
            }
            Ok(Event::End(_)) => current = None,
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(Value::Object(map))
}

fn epub_rootfile(xml: &[u8]) -> Result<String, String> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) | Ok(Event::Empty(event)) => {
                if event.local_name().as_ref() == "rootfile" {
                    for attribute in event.attributes().with_checks(false) {
                        let attribute = attribute.map_err(|error| error.to_string())?;
                        if attribute.key.as_ref() == "full-path" {
                            return Ok(attribute
                                .normalized_value(XmlVersion::Implicit1_0)
                                .map_err(|error| error.to_string())?
                                .to_string());
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("epub: container.xml has no rootfile".to_string())
}

type EpubPackage = (Value, BTreeMap<String, String>, Vec<String>);

fn epub_package(xml: &[u8]) -> Result<EpubPackage, String> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut metadata = Map::new();
    let mut manifest = BTreeMap::new();
    let mut spine = Vec::new();
    let mut current_metadata: Option<String> = None;
    let metadata_keys: BTreeSet<&str> = [
        "title",
        "creator",
        "language",
        "identifier",
        "date",
        "publisher",
        "description",
        "rights",
        "subject",
    ]
    .into_iter()
    .collect();
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) | Ok(Event::Empty(event)) => {
                let local = event.local_name().as_ref().to_string();
                if metadata_keys.contains(local.as_str()) {
                    current_metadata = Some(local.clone());
                } else if local == "item" {
                    let mut id = String::new();
                    let mut href = String::new();
                    for attribute in event.attributes().with_checks(false) {
                        let attribute = attribute.map_err(|error| error.to_string())?;
                        let value = attribute
                            .normalized_value(XmlVersion::Implicit1_0)
                            .map_err(|error| error.to_string())?
                            .to_string();
                        match attribute.key.as_ref() {
                            "id" => id = value,
                            "href" => href = value,
                            _ => {}
                        }
                    }
                    if !id.is_empty() && !href.is_empty() {
                        manifest.insert(id, href);
                    }
                } else if local == "itemref" {
                    for attribute in event.attributes().with_checks(false) {
                        let attribute = attribute.map_err(|error| error.to_string())?;
                        if attribute.key.as_ref() == "idref" {
                            spine.push(
                                attribute
                                    .normalized_value(XmlVersion::Implicit1_0)
                                    .map_err(|error| error.to_string())?
                                    .to_string(),
                            );
                        }
                    }
                }
            }
            Ok(Event::Text(event)) => {
                if let Some(name) = &current_metadata {
                    let text = event.xml_content(XmlVersion::Implicit1_0);
                    metadata
                        .entry(name.clone())
                        .or_insert_with(|| Value::String(String::new()));
                    if let Some(Value::String(existing)) = metadata.get_mut(name) {
                        existing.push_str(&text);
                    }
                }
            }
            Ok(Event::End(_)) => current_metadata = None,
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok((Value::Object(metadata), manifest, spine))
}

fn resolve_path(base: &str, href: &str) -> String {
    let href = href.split('#').next().unwrap_or(href);
    let mut parts: Vec<&str> = if href.starts_with('/') {
        Vec::new()
    } else {
        base.split('/').filter(|part| !part.is_empty()).collect()
    };
    for part in href.split('/') {
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

fn html_text(data: &[u8]) -> String {
    let text = String::from_utf8_lossy(data);
    let document = Html::parse_document(&text);
    document
        .root_element()
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn ensure_newline(text: &mut String) {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
}

fn rtf_text(bytes: &[u8]) -> Result<(String, BTreeSet<String>), String> {
    let source: Vec<char> = bytes.iter().map(|byte| char::from(*byte)).collect();
    let mut output = String::new();
    let mut control_words = BTreeSet::new();
    let mut stack: Vec<bool> = Vec::new();
    let mut skip = false;
    let mut index = 0usize;
    while index < source.len() {
        let ch = source[index];
        match ch {
            '{' => {
                stack.push(skip);
                index += 1;
            }
            '}' => {
                skip = stack.pop().unwrap_or(false);
                index += 1;
            }
            '\\' => {
                index += 1;
                if index >= source.len() {
                    break;
                }
                let next = source[index];
                if next == '*' {
                    skip = true;
                    index += 1;
                    continue;
                }
                if next == '\'' {
                    if index + 2 < source.len() {
                        let hex_text: String = source[index + 1..=index + 2].iter().collect();
                        if let Ok(byte) = u8::from_str_radix(&hex_text, 16)
                            && !skip
                        {
                            output.push(windows_1252(byte));
                        }
                        index += 3;
                    } else {
                        index = source.len();
                    }
                    continue;
                }
                if next == '\\' || next == '{' || next == '}' {
                    if !skip {
                        output.push(next);
                    }
                    index += 1;
                    continue;
                }
                if next == '~' {
                    if !skip {
                        output.push(' ');
                    }
                    index += 1;
                    continue;
                }
                if next == '-' || next == '_' {
                    index += 1;
                    continue;
                }
                let word_start = index;
                while index < source.len() && source[index].is_ascii_alphabetic() {
                    index += 1;
                }
                let word: String = source[word_start..index].iter().collect();
                if word.is_empty() {
                    index += 1;
                    continue;
                }
                let mut parameter = String::new();
                if index < source.len() && (source[index] == '-' || source[index].is_ascii_digit())
                {
                    if source[index] == '-' {
                        parameter.push('-');
                        index += 1;
                    }
                    while index < source.len() && source[index].is_ascii_digit() {
                        parameter.push(source[index]);
                        index += 1;
                    }
                }
                control_words.insert(format!("{word}{parameter}"));
                if index < source.len() && source[index] == ' ' {
                    index += 1;
                }
                if is_rtf_destination(&word) {
                    skip = true;
                }
                if !skip {
                    match word.as_str() {
                        "par" | "line" | "page" => output.push('\n'),
                        "tab" => output.push('\t'),
                        "u" => {
                            if let Ok(value) = parameter.parse::<i32>() {
                                let code = if value < 0 {
                                    (value + 65_536) as u32
                                } else {
                                    value as u32
                                };
                                if let Some(character) = char::from_u32(code) {
                                    output.push(character);
                                }
                                if index < source.len() && source[index] == '?' {
                                    index += 1;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {
                if !skip {
                    output.push(ch);
                }
                index += 1;
            }
        }
    }
    Ok((output, control_words))
}

fn is_rtf_destination(word: &str) -> bool {
    matches!(
        word,
        "fonttbl"
            | "colortbl"
            | "stylesheet"
            | "info"
            | "pict"
            | "object"
            | "header"
            | "footer"
            | "footnote"
            | "annotation"
            | "field"
            | "fldinst"
            | "listtable"
            | "listoverridetable"
            | "revtbl"
            | "rsidtbl"
            | "generator"
            | "xmlnstbl"
            | "themedata"
            | "colorschememapping"
            | "datastore"
            | "latentstyles"
            | "pgptbl"
            | "pnseclvl"
            | "filetbl"
    )
}

fn windows_1252(byte: u8) -> char {
    let bytes = [byte];
    let (text, _, _) = encoding_rs::WINDOWS_1252.decode(&bytes);
    text.chars().next().unwrap_or('\u{fffd}')
}

fn parse_mime_entity(text: &str, depth: usize) -> Result<Value, String> {
    if depth > 32 {
        return Err("eml: MIME nesting too deep".to_string());
    }
    let (header_block, body) = split_headers(text);
    let headers = parse_headers(&header_block);
    let (content_type, params) = headers
        .get("content-type")
        .map(|value| parse_content_type(value))
        .unwrap_or_else(|| ("text/plain".to_string(), BTreeMap::new()));
    let filename = headers
        .get("content-disposition")
        .and_then(|value| parse_parameter(value, "filename"))
        .or_else(|| params.get("name").cloned());
    let transfer = headers
        .get("content-transfer-encoding")
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let charset = params.get("charset").cloned();
    let mut part = Map::new();
    part.insert("headers".to_string(), json!(headers));
    part.insert(
        "content_type".to_string(),
        Value::String(content_type.clone()),
    );
    if let Some(filename) = &filename {
        part.insert("filename".to_string(), Value::String(filename.clone()));
    }
    if content_type.starts_with("multipart/") {
        let Some(boundary) = params.get("boundary") else {
            part.insert("body".to_string(), Value::String(body));
            return Ok(Value::Object(part));
        };
        let mut children = Vec::new();
        for piece in body.split(&format!("--{boundary}")) {
            let piece = piece.trim_start_matches(['\r', '\n']);
            if piece.is_empty() || piece.starts_with("--") {
                continue;
            }
            children.push(parse_mime_entity(piece, depth + 1)?);
        }
        part.insert("parts".to_string(), Value::Array(children));
        return Ok(Value::Object(part));
    }
    let decoded = decode_body(&body, &transfer);
    let text_body = if content_type.starts_with("text/") {
        Some(decode_text(&decoded, charset.as_deref()))
    } else {
        None
    };
    part.insert("size".to_string(), json!(decoded.len()));
    part.insert(
        "sha256".to_string(),
        Value::String(hex(&Sha256::digest(&decoded))),
    );
    if let Some(text_body) = text_body {
        part.insert(
            "text".to_string(),
            Value::String(text_body.trim().to_string()),
        );
    }
    if filename.is_some() || !content_type.starts_with("text/") {
        part.insert(
            "attachment".to_string(),
            Value::Bool(filename.is_some() || !content_type.starts_with("text/")),
        );
    }
    Ok(Value::Object(part))
}

fn collect_mail_parts(
    part: &Value,
    texts: &mut Vec<String>,
    html: &mut Vec<String>,
    attachments: &mut Vec<Value>,
) {
    if let Some(children) = part.get("parts").and_then(Value::as_array) {
        for child in children {
            collect_mail_parts(child, texts, html, attachments);
        }
        return;
    }
    let content_type = part
        .get("content_type")
        .and_then(Value::as_str)
        .unwrap_or("application/octet-stream");
    if let Some(text) = part.get("text").and_then(Value::as_str) {
        if content_type.starts_with("text/html") {
            html.push(text.to_string());
        } else if content_type.starts_with("text/") {
            texts.push(text.to_string());
        }
        return;
    }
    if part
        .get("attachment")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        attachments.push(json!({
            "filename": part.get("filename").cloned().unwrap_or(Value::Null),
            "content_type": content_type,
            "size": part.get("size").cloned().unwrap_or(Value::Null),
            "sha256": part.get("sha256").cloned().unwrap_or(Value::Null),
        }));
    }
}

fn split_headers(text: &str) -> (String, String) {
    if let Some(index) = text.find("\r\n\r\n") {
        return (text[..index].to_string(), text[index + 4..].to_string());
    }
    if let Some(index) = text.find("\n\n") {
        return (text[..index].to_string(), text[index + 2..].to_string());
    }
    (text.to_string(), String::new())
}

fn parse_headers(block: &str) -> BTreeMap<String, String> {
    let mut headers: BTreeMap<String, String> = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in block.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(key) = &current
                && let Some(value) = headers.get_mut(key)
            {
                value.push(' ');
                value.push_str(line.trim());
            }
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        headers.insert(key.clone(), value.trim().to_string());
        current = Some(key);
    }
    headers
}

fn parse_content_type(value: &str) -> (String, BTreeMap<String, String>) {
    let mut parts = value.split(';');
    let content_type = parts
        .next()
        .unwrap_or("text/plain")
        .trim()
        .to_ascii_lowercase();
    let mut params = BTreeMap::new();
    for part in parts {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        params.insert(
            key.trim().to_ascii_lowercase(),
            value.trim().trim_matches('"').to_string(),
        );
    }
    (content_type, params)
}

fn parse_parameter(value: &str, key: &str) -> Option<String> {
    for part in value.split(';') {
        let Some((name, value)) = part.split_once('=') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case(key) {
            return Some(value.trim().trim_matches('"').to_string());
        }
    }
    None
}

fn decode_body(body: &str, transfer: &str) -> Vec<u8> {
    match transfer {
        "base64" => {
            let compact: String = body.chars().filter(|ch| !ch.is_whitespace()).collect();
            BASE64
                .decode(compact)
                .unwrap_or_else(|_| body.as_bytes().to_vec())
        }
        "quoted-printable" => decode_quoted_printable(body),
        _ => body.as_bytes().to_vec(),
    }
}

fn decode_quoted_printable(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut output = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'=' {
            if index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                index += 2;
                continue;
            }
            if index + 2 < bytes.len()
                && let (Some(high), Some(low)) =
                    (hex_digit(bytes[index + 1]), hex_digit(bytes[index + 2]))
            {
                output.push(high * 16 + low);
                index += 3;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    output
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn decode_text(bytes: &[u8], charset: Option<&str>) -> String {
    let encoding = charset
        .and_then(|name| encoding_rs::Encoding::for_label(name.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);
    let (text, _, _) = encoding.decode(bytes);
    text.into_owned()
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
#[path = "../../tests/unit/diff/documents_extra.rs"]
mod tests;
