use crate::model::{FileKind, StructuredChange, StructuredChangeKind, StructuredDiff};
use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};
use serde_json::{Map, Value};
use similar::{Algorithm, DiffOp, capture_diff_slices};
use std::collections::BTreeSet;

pub fn format_label(kind: FileKind) -> &'static str {
    match kind {
        FileKind::Json => "json",
        FileKind::Yaml => "yaml",
        FileKind::Toml => "toml",
        FileKind::Ini => "ini",
        FileKind::Xml => "xml",
        FileKind::Html => "html",
        FileKind::Plist => "plist",
        FileKind::Pickle => "pickle",
        FileKind::Mo => "gettext",
        FileKind::Wasm => "wasm",
        FileKind::JavaClass => "java-class",
        FileKind::AndroidXml => "android-xml",
        FileKind::GitIndex => "git-index",
        FileKind::Ico => "ico",
        FileKind::SshPublicKey => "openssh-key",
        FileKind::Sqlite => "sqlite",
        FileKind::Executable => "executable",
        FileKind::Font => "font",
        FileKind::Notebook => "notebook",
        FileKind::Parquet => "parquet",
        FileKind::Avro => "avro",
        FileKind::Orc => "orc",
        FileKind::Cbor => "cbor",
        FileKind::MessagePack => "messagepack",
        FileKind::Protobuf => "protobuf",
        FileKind::Rtf => "rtf",
        FileKind::Eml => "eml",
        FileKind::Odt => "odt",
        FileKind::Odp => "odp",
        FileKind::Epub => "epub",
        FileKind::Squashfs => "squashfs",
        FileKind::Snap => "snap",
        FileKind::AppImage => "appimage",
        FileKind::Flatpak => "flatpak",
        FileKind::Dmg => "dmg",
        FileKind::Msix => "msix",
        FileKind::Aab => "aab",
        FileKind::Ext4 => "ext4",
        _ => "structured",
    }
}

fn parse_json(bytes: &[u8]) -> std::result::Result<Value, String> {
    match serde_json::from_slice(bytes) {
        Ok(value) => Ok(value),
        Err(json_error) => {
            let text = std::str::from_utf8(bytes).map_err(|_| json_error.to_string())?;
            json5::from_str(text)
                .map_err(|json5_error| format!("JSON: {json_error}; JSON5: {json5_error}"))
        }
    }
}

fn parse_html(bytes: &[u8]) -> std::result::Result<Value, String> {
    let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    let document = scraper::Html::parse_document(text);
    let root = document.root_element();
    Ok(element_to_value(root))
}

fn element_to_value(element: scraper::ElementRef<'_>) -> Value {
    let mut map = Map::new();
    map.insert(
        "tag".to_string(),
        Value::String(element.value().name().to_string()),
    );

    let mut attributes = Map::new();
    for (key, value) in element.value().attrs() {
        attributes.insert(key.to_string(), Value::String(value.to_string()));
    }
    if !attributes.is_empty() {
        map.insert("attributes".to_string(), Value::Object(attributes));
    }

    let mut text = String::new();
    let mut children = Vec::new();
    for child in element.children() {
        match child.value() {
            scraper::node::Node::Text(value) => append_normalized_text(&mut text, value),
            scraper::node::Node::Element(_) => {
                if let Some(element) = scraper::ElementRef::wrap(child) {
                    children.push(element_to_value(element));
                }
            }
            _ => {}
        }
    }
    map.insert("text".to_string(), Value::String(text));
    map.insert("children".to_string(), Value::Array(children));
    Value::Object(map)
}

fn append_normalized_text(text: &mut String, value: &str) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }
    if !text.is_empty() {
        text.push(' ');
    }
    text.push_str(trimmed);
}

fn parse_ini(bytes: &[u8]) -> std::result::Result<Value, String> {
    let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    let mut root = Map::new();
    let mut sections: std::collections::BTreeMap<String, Map<String, Value>> =
        std::collections::BTreeMap::new();
    let mut current_section: Option<String> = None;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let name = line[1..line.len() - 1].trim().to_string();
            sections.entry(name.clone()).or_default();
            current_section = Some(name);
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().to_string();
        let value = parse_ini_scalar(value.trim());
        if let Some(section) = &current_section {
            sections
                .entry(section.clone())
                .or_default()
                .insert(key, value);
        } else {
            root.insert(key, value);
        }
    }

    for (name, values) in sections {
        root.insert(name, Value::Object(values));
    }
    Ok(Value::Object(root))
}

fn parse_ini_scalar(raw: &str) -> Value {
    let raw = raw
        .split_once(';')
        .map(|(value, _)| value.trim())
        .unwrap_or(raw)
        .trim();
    if raw.len() >= 2
        && ((raw.starts_with('"') && raw.ends_with('"'))
            || (raw.starts_with('\'') && raw.ends_with('\'')))
    {
        return Value::String(raw[1..raw.len() - 1].to_string());
    }
    match raw {
        "true" | "yes" | "on" => Value::Bool(true),
        "false" | "no" | "off" => Value::Bool(false),
        _ => {
            if let Ok(value) = raw.parse::<i64>() {
                Value::Number(value.into())
            } else if let Ok(value) = raw.parse::<f64>() {
                serde_json::Number::from_f64(value)
                    .map(Value::Number)
                    .unwrap_or_else(|| Value::String(raw.to_string()))
            } else {
                Value::String(raw.to_string())
            }
        }
    }
}

pub fn parse_structured(kind: FileKind, bytes: &[u8]) -> std::result::Result<Value, String> {
    match kind {
        FileKind::Json => parse_json(bytes),
        FileKind::Yaml => parse_yaml(bytes),
        FileKind::Toml => parse_toml(bytes),
        FileKind::Ini => parse_ini(bytes),
        FileKind::Xml => parse_xml(bytes),
        FileKind::Html => parse_html(bytes),
        FileKind::Plist => parse_plist(bytes),
        FileKind::Pickle => parse_pickle(bytes),
        FileKind::Sqlite => crate::diff::sqlite::parse_sqlite(bytes),
        FileKind::Executable => crate::diff::executable::parse_executable(bytes),
        FileKind::Mo => crate::diff::gettext::parse_mo(bytes),
        FileKind::Wasm => crate::diff::wasm::parse_wasm(bytes),
        FileKind::JavaClass => crate::diff::java_class::parse_class(bytes),
        FileKind::AndroidXml => crate::diff::android_xml::parse_android_xml(bytes),
        FileKind::GitIndex => crate::diff::git_index::parse_git_index(bytes),
        FileKind::Ico => crate::diff::ico::parse_ico(bytes),
        FileKind::SshPublicKey => crate::diff::openssh_key::parse_public_key(bytes),
        FileKind::Font => crate::diff::font::parse_font(bytes),
        FileKind::Notebook => serde_json::from_slice(bytes).map_err(|error| error.to_string()),
        FileKind::Cbor => crate::diff::serialization::parse_cbor(bytes),
        FileKind::MessagePack => crate::diff::serialization::parse_messagepack(bytes),
        FileKind::Protobuf => crate::diff::serialization::parse_protobuf(bytes),
        FileKind::Parquet => crate::diff::tabular::parse_parquet(bytes),
        FileKind::Avro => crate::diff::tabular::parse_avro(bytes),
        FileKind::Orc => crate::diff::tabular::parse_orc(bytes),
        FileKind::Rtf => crate::diff::documents_extra::parse_rtf(bytes),
        FileKind::Eml => crate::diff::documents_extra::parse_eml(bytes),
        FileKind::Odt => crate::diff::documents_extra::parse_odt(bytes),
        FileKind::Odp => crate::diff::documents_extra::parse_odp(bytes),
        FileKind::Epub => crate::diff::documents_extra::parse_epub(bytes),
        FileKind::Squashfs => crate::diff::packages::parse_squashfs(bytes),
        FileKind::Snap => crate::diff::packages::parse_snap(bytes),
        FileKind::AppImage => crate::diff::packages::parse_appimage(bytes),
        FileKind::Flatpak => crate::diff::packages::parse_flatpak(bytes),
        FileKind::Dmg => crate::diff::packages::parse_dmg(bytes),
        FileKind::Msix => crate::diff::packages::parse_msix(bytes),
        FileKind::Aab => crate::diff::packages::parse_aab(bytes),
        FileKind::Ext4 => crate::diff::packages::parse_ext4(bytes),
        _ => Err(format!("{} is not a structured format", kind.label())),
    }
}

fn parse_yaml(bytes: &[u8]) -> std::result::Result<Value, String> {
    let value: serde_yaml::Value =
        serde_yaml::from_slice(bytes).map_err(|error| error.to_string())?;
    serde_json::to_value(value).map_err(|error| error.to_string())
}

fn parse_plist(bytes: &[u8]) -> std::result::Result<Value, String> {
    let cursor = std::io::Cursor::new(bytes);
    let value = plist::Value::from_reader(cursor).map_err(|error| error.to_string())?;
    serde_json::to_value(value).map_err(|error| error.to_string())
}

fn parse_pickle(bytes: &[u8]) -> std::result::Result<Value, String> {
    match serde_pickle::from_slice::<serde_pickle::Value>(bytes, serde_pickle::DeOptions::new()) {
        Ok(value) => {
            let mut value = pickle_to_json(&value);
            append_pickle_global_strings(&mut value, bytes);
            Ok(value)
        }
        Err(error) => Ok(pickle_error_value(bytes, &error.to_string())),
    }
}

fn pickle_error_value(bytes: &[u8], error: &str) -> Value {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    let mut object = Map::new();
    object.insert(
        "$pickle_error".to_string(),
        Value::String(error.to_string()),
    );
    object.insert(
        "$bytes_hash".to_string(),
        Value::String(format!("{:016x}", hasher.finish())),
    );
    object.insert("$size".to_string(), Value::Number(bytes.len().into()));
    let strings = pickle_readable_strings(bytes);
    if !strings.is_empty() {
        object.insert("$readable_strings".to_string(), Value::Array(strings));
    }
    Value::Object(object)
}

fn pickle_to_json(value: &serde_pickle::Value) -> Value {
    use serde_pickle::Value as Pickle;
    match value {
        Pickle::None => Value::Null,
        Pickle::Bool(value) => Value::Bool(*value),
        Pickle::I64(value) => Value::Number((*value).into()),
        Pickle::Int(value) => Value::String(value.to_string()),
        Pickle::F64(value) => serde_json::Number::from_f64(*value)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        Pickle::Bytes(bytes) => {
            let mut hex = String::with_capacity(bytes.len() * 2);
            for byte in bytes {
                use std::fmt::Write as _;
                let _ = write!(hex, "{byte:02x}");
            }
            let mut object = Map::new();
            object.insert("$bytes".to_string(), Value::String(hex));
            Value::Object(object)
        }
        Pickle::String(value) => Value::String(value.clone()),
        Pickle::List(values) | Pickle::Tuple(values) => {
            Value::Array(values.iter().map(pickle_to_json).collect())
        }
        Pickle::Set(values) => Value::Array(values.iter().map(pickle_hashable_to_json).collect()),
        Pickle::FrozenSet(values) => {
            Value::Array(values.iter().map(pickle_hashable_to_json).collect())
        }
        Pickle::Dict(values) => {
            if values
                .keys()
                .all(|key| matches!(key, serde_pickle::HashableValue::String(_)))
            {
                let mut object = Map::new();
                for (key, value) in values {
                    let serde_pickle::HashableValue::String(key) = key else {
                        unreachable!();
                    };
                    object.insert(key.clone(), pickle_to_json(value));
                }
                Value::Object(object)
            } else {
                Value::Array(
                    values
                        .iter()
                        .map(|(key, value)| {
                            Value::Array(vec![pickle_hashable_to_json(key), pickle_to_json(value)])
                        })
                        .collect(),
                )
            }
        }
    }
}

fn append_pickle_global_strings(value: &mut Value, bytes: &[u8]) {
    // serde-pickle decodes unknown global references to an empty object. Add
    // the printable strings from the pickle stream so class/module names such
    // as __main__.Widget still appear in the readable diff.
    if !bytes.contains(&0x93) {
        return;
    }
    let Value::Object(object) = value else {
        return;
    };
    let strings = pickle_readable_strings(bytes);
    if !strings.is_empty() {
        object.insert("$readable_strings".to_string(), Value::Array(strings));
    }
}

fn pickle_readable_strings(bytes: &[u8]) -> Vec<Value> {
    let mut strings = Vec::new();
    let mut start = None;
    for (index, byte) in bytes.iter().enumerate() {
        if byte.is_ascii_graphic() || *byte == b' ' {
            start.get_or_insert(index);
        } else if let Some(start) = start.take()
            && index - start >= 3
        {
            strings.push(Value::String(
                String::from_utf8_lossy(&bytes[start..index]).to_string(),
            ));
        }
    }
    if let Some(start) = start
        && bytes.len() - start >= 3
    {
        strings.push(Value::String(
            String::from_utf8_lossy(&bytes[start..]).to_string(),
        ));
    }
    strings.sort_by(|left, right| left.as_str().cmp(&right.as_str()));
    strings.dedup();
    strings
}

fn pickle_hashable_to_json(value: &serde_pickle::HashableValue) -> Value {
    use serde_pickle::HashableValue as Pickle;
    match value {
        Pickle::None => Value::Null,
        Pickle::Bool(value) => Value::Bool(*value),
        Pickle::I64(value) => Value::Number((*value).into()),
        Pickle::Int(value) => Value::String(value.to_string()),
        Pickle::F64(value) => serde_json::Number::from_f64(*value)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        Pickle::Bytes(bytes) => {
            let mut hex = String::with_capacity(bytes.len() * 2);
            for byte in bytes {
                use std::fmt::Write as _;
                let _ = write!(hex, "{byte:02x}");
            }
            let mut object = Map::new();
            object.insert("$bytes".to_string(), Value::String(hex));
            Value::Object(object)
        }
        Pickle::String(value) => Value::String(value.clone()),
        Pickle::Tuple(values) => Value::Array(values.iter().map(pickle_hashable_to_json).collect()),
        Pickle::FrozenSet(values) => {
            Value::Array(values.iter().map(pickle_hashable_to_json).collect())
        }
    }
}

fn parse_toml(bytes: &[u8]) -> std::result::Result<Value, String> {
    let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    let value: toml::Value = toml::from_str(text).map_err(|error| error.to_string())?;
    serde_json::to_value(value).map_err(|error| error.to_string())
}

fn parse_xml(bytes: &[u8]) -> std::result::Result<Value, String> {
    let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(false);

    let mut stack: Vec<Value> = Vec::new();
    let mut root: Option<Value> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(start)) => {
                stack.push(element_value(&start)?);
            }
            Ok(Event::Empty(start)) => {
                let node = element_value(&start)?;
                attach(node, &mut stack, &mut root)?;
            }
            Ok(Event::Text(event)) => {
                let text = event.xml_content(XmlVersion::Implicit1_0);
                if let Some(node) = stack.last_mut() {
                    append_text(node, &text);
                }
            }
            Ok(Event::CData(event)) => {
                if let Some(node) = stack.last_mut() {
                    append_text(node, event.as_ref());
                }
            }
            Ok(Event::End(_)) => {
                if let Some(node) = stack.pop() {
                    attach(node, &mut stack, &mut root)?;
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(error.to_string()),
        }
    }

    if !stack.is_empty() {
        return Err("unclosed XML elements".to_string());
    }
    root.ok_or_else(|| "no XML root element".to_string())
}

fn element_value(start: &quick_xml::events::BytesStart<'_>) -> std::result::Result<Value, String> {
    let name = start.name().as_ref().to_string();
    let mut attributes = Map::new();
    for attribute in start.attributes().with_checks(false) {
        let attribute = attribute.map_err(|error| error.to_string())?;
        let key = attribute.key.as_ref().to_string();
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|error| error.to_string())?
            .into_owned();
        attributes.insert(key, Value::String(value));
    }

    let mut map = Map::new();
    map.insert("name".to_string(), Value::String(name));
    if !attributes.is_empty() {
        map.insert("attributes".to_string(), Value::Object(attributes));
    }
    map.insert("text".to_string(), Value::String(String::new()));
    map.insert("children".to_string(), Value::Array(Vec::new()));
    Ok(Value::Object(map))
}

fn append_text(node: &mut Value, text: &str) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    if let Some(object) = node.as_object_mut()
        && let Some(Value::String(existing)) = object.get_mut("text")
    {
        if !existing.is_empty() {
            existing.push(' ');
        }
        existing.push_str(trimmed);
    }
}

fn attach(
    node: Value,
    stack: &mut [Value],
    root: &mut Option<Value>,
) -> std::result::Result<(), String> {
    if let Some(parent) = stack.last_mut() {
        if let Some(children) = parent
            .as_object_mut()
            .and_then(|object| object.get_mut("children"))
            .and_then(Value::as_array_mut)
        {
            children.push(node);
        }
    } else if root.is_none() {
        *root = Some(node);
    } else {
        return Err("multiple XML root elements".to_string());
    }
    Ok(())
}

pub fn compare_values(
    format: &str,
    left: &Value,
    right: &Value,
    max_changes: usize,
) -> StructuredDiff {
    let mut changes = Vec::new();
    let mut truncated = false;
    diff_value(
        "$",
        left,
        right,
        &mut ChangeSink {
            changes: &mut changes,
            max_changes,
            truncated: &mut truncated,
        },
    );
    StructuredDiff {
        format: format.to_string(),
        changes,
        truncated,
    }
}

pub fn looks_like_openapi(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    (object.contains_key("openapi") || object.contains_key("swagger"))
        && object.get("paths").is_some_and(Value::is_object)
}

pub fn compare_openapi(left: &Value, right: &Value, max_changes: usize) -> StructuredDiff {
    let mut diff = compare_values("openapi", left, right, max_changes);
    for change in &mut diff.changes {
        change.breaking = openapi_breaking(change);
    }
    diff
}

fn openapi_breaking(change: &StructuredChange) -> bool {
    let in_paths = change.path.starts_with("$.paths");
    let in_schema =
        change.path.contains(".schemas.") || change.path.starts_with("$.components.schemas");
    if !in_paths && !in_schema {
        return false;
    }
    let operation_path = change.path.rsplit('.').next().is_some_and(|segment| {
        matches!(
            segment,
            "get" | "put" | "post" | "delete" | "options" | "head" | "patch" | "trace"
        )
    });
    let required_member = change.path.contains(".required[") || change.path.ends_with(".required");
    let property = change.path.contains(".properties.") || change.path.ends_with(".properties");
    let type_change = change.path.ends_with(".type")
        && change.left.as_ref().and_then(Value::as_str).is_some()
        && change.right.as_ref().and_then(Value::as_str).is_some()
        && change.left != change.right;

    match change.kind {
        StructuredChangeKind::Removed => {
            if operation_path
                || change.path.contains(".responses.")
                || required_member
                || (in_schema && property)
            {
                return true;
            }
            change.left.as_ref().is_some_and(|value| {
                value.as_object().is_some_and(|object| {
                    object.keys().any(|key| {
                        matches!(
                            key.as_str(),
                            "get"
                                | "put"
                                | "post"
                                | "delete"
                                | "options"
                                | "head"
                                | "patch"
                                | "trace"
                        )
                    })
                })
            })
        }
        StructuredChangeKind::Added => {
            if required_member || (in_schema && property) {
                return true;
            }
            change.right.as_ref().is_some_and(|value| {
                let required = value.get("required").and_then(Value::as_bool) == Some(true);
                let required_parameter = value.as_array().is_some_and(|items| {
                    items
                        .iter()
                        .any(|item| item.get("required").and_then(Value::as_bool) == Some(true))
                });
                (required
                    && (change.path.contains(".parameters")
                        || change.path.ends_with(".required")
                        || change.path.ends_with(".requestBody")))
                    || (required_parameter && change.path.contains(".parameters"))
            })
        }
        StructuredChangeKind::Changed => {
            type_change
                || (change.path.ends_with(".required")
                    && change.right.as_ref().and_then(Value::as_bool) == Some(true))
        }
    }
}

struct ChangeSink<'a> {
    changes: &'a mut Vec<StructuredChange>,
    max_changes: usize,
    truncated: &'a mut bool,
}

fn diff_value(path: &str, left: &Value, right: &Value, sink: &mut ChangeSink<'_>) {
    if sink.changes.len() >= sink.max_changes {
        *sink.truncated = true;
        return;
    }

    match (left, right) {
        (Value::Object(left_map), Value::Object(right_map)) => {
            let mut keys: BTreeSet<&String> = left_map.keys().collect();
            keys.extend(right_map.keys());
            for key in keys {
                let child_path = format!("{path}.{key}");
                match (left_map.get(key), right_map.get(key)) {
                    (Some(left_value), Some(right_value)) => {
                        diff_value(&child_path, left_value, right_value, sink);
                    }
                    (Some(left_value), None) => sink.push(StructuredChange {
                        path: child_path,
                        kind: StructuredChangeKind::Removed,
                        left: Some(left_value.clone()),
                        right: None,
                        breaking: false,
                    }),
                    (None, Some(right_value)) => sink.push(StructuredChange {
                        path: child_path,
                        kind: StructuredChangeKind::Added,
                        left: None,
                        right: Some(right_value.clone()),
                        breaking: false,
                    }),
                    (None, None) => {}
                }
            }
        }
        (Value::Array(left_array), Value::Array(right_array)) => {
            diff_array(path, left_array, right_array, sink);
        }
        _ if left == right => {}
        _ => sink.push(StructuredChange {
            path: path.to_string(),
            kind: StructuredChangeKind::Changed,
            left: Some(left.clone()),
            right: Some(right.clone()),
            breaking: false,
        }),
    }
}

fn diff_array(path: &str, left: &[Value], right: &[Value], sink: &mut ChangeSink<'_>) {
    if left.is_empty() && right.is_empty() {
        return;
    }
    if sink.changes.len() >= sink.max_changes {
        *sink.truncated = true;
        return;
    }
    if left.is_empty() {
        for (index, value) in right.iter().enumerate() {
            sink.push(StructuredChange {
                path: format!("{path}[{index}]"),
                kind: StructuredChangeKind::Added,
                left: None,
                right: Some(value.clone()),
                breaking: false,
            });
        }
        return;
    }
    if right.is_empty() {
        for (index, value) in left.iter().enumerate() {
            sink.push(StructuredChange {
                path: format!("{path}[{index}]"),
                kind: StructuredChangeKind::Removed,
                left: Some(value.clone()),
                right: None,
                breaking: false,
            });
        }
        return;
    }
    if left.len().saturating_mul(right.len()) > 10_000 {
        diff_array_myers(path, left, right, sink);
        return;
    }

    for (left_index, right_index) in align_values(left, right) {
        match (left_index, right_index) {
            (Some(left_index), Some(right_index)) => {
                diff_value(
                    &format!("{path}[{right_index}]"),
                    &left[left_index],
                    &right[right_index],
                    sink,
                );
            }
            (Some(left_index), None) => sink.push(StructuredChange {
                path: format!("{path}[{left_index}]"),
                kind: StructuredChangeKind::Removed,
                left: Some(left[left_index].clone()),
                right: None,
                breaking: false,
            }),
            (None, Some(right_index)) => sink.push(StructuredChange {
                path: format!("{path}[{right_index}]"),
                kind: StructuredChangeKind::Added,
                left: None,
                right: Some(right[right_index].clone()),
                breaking: false,
            }),
            (None, None) => {}
        }
    }
}

fn align_values(left: &[Value], right: &[Value]) -> Vec<(Option<usize>, Option<usize>)> {
    const GAP: i32 = -200;
    let width = right.len() + 1;
    let mut scores = vec![0i32; (left.len() + 1) * width];
    for left_index in 1..=left.len() {
        scores[left_index * width] = left_index as i32 * GAP;
    }
    for (right_index, score) in scores.iter_mut().enumerate().take(right.len() + 1).skip(1) {
        *score = right_index as i32 * GAP;
    }
    for left_index in 1..=left.len() {
        for right_index in 1..=right.len() {
            let mut best = (scores[(left_index - 1) * width + right_index] + GAP)
                .max(scores[left_index * width + right_index - 1] + GAP);
            if let Some(score) = value_match_score(&left[left_index - 1], &right[right_index - 1]) {
                best = best.max(scores[(left_index - 1) * width + right_index - 1] + score);
            }
            scores[left_index * width + right_index] = best;
        }
    }

    let mut pairs = Vec::new();
    let mut left_index = left.len();
    let mut right_index = right.len();
    while left_index > 0 || right_index > 0 {
        if left_index > 0
            && right_index > 0
            && let Some(score) = value_match_score(&left[left_index - 1], &right[right_index - 1])
            && scores[left_index * width + right_index]
                == scores[(left_index - 1) * width + right_index - 1] + score
        {
            pairs.push((Some(left_index - 1), Some(right_index - 1)));
            left_index -= 1;
            right_index -= 1;
            continue;
        }
        if left_index > 0
            && scores[left_index * width + right_index]
                == scores[(left_index - 1) * width + right_index] + GAP
        {
            pairs.push((Some(left_index - 1), None));
            left_index -= 1;
        } else if right_index > 0 {
            pairs.push((None, Some(right_index - 1)));
            right_index -= 1;
        } else {
            break;
        }
    }
    pairs.reverse();
    pairs
}

fn value_match_score(left: &Value, right: &Value) -> Option<i32> {
    if left == right {
        return Some(1_000);
    }
    match (left, right) {
        (Value::Object(_), Value::Object(_)) | (Value::Array(_), Value::Array(_)) => {
            let left_tokens = similarity_tokens(left);
            let right_tokens = similarity_tokens(right);
            let max_len = left_tokens.len().max(right_tokens.len()).max(1);
            let shared = shared_token_count(&left_tokens, &right_tokens);
            Some(50 + (shared * 900 / max_len) as i32)
        }
        (Value::String(_), Value::String(_))
        | (Value::Number(_), Value::Number(_))
        | (Value::Bool(_), Value::Bool(_))
        | (Value::Null, Value::Null) => Some(30),
        _ => None,
    }
}

fn similarity_tokens(value: &Value) -> Vec<String> {
    let mut tokens = Vec::new();
    collect_similarity_tokens(value, &mut tokens);
    tokens.truncate(1_024);
    tokens
}

fn collect_similarity_tokens(value: &Value, tokens: &mut Vec<String>) {
    if tokens.len() >= 1_024 {
        return;
    }
    match value {
        Value::Null => tokens.push("z".to_string()),
        Value::Bool(value) => tokens.push(format!("b:{value}")),
        Value::Number(value) => tokens.push(format!("n:{value}")),
        Value::String(value) => tokens.push(format!("s:{value}")),
        Value::Array(values) => {
            for value in values {
                collect_similarity_tokens(value, tokens);
            }
        }
        Value::Object(map) => {
            for (key, value) in map {
                tokens.push(format!("k:{key}"));
                collect_similarity_tokens(value, tokens);
            }
        }
    }
}

fn shared_token_count(left: &[String], right: &[String]) -> usize {
    let mut left_sorted: Vec<&str> = left.iter().map(String::as_str).collect();
    let mut right_sorted: Vec<&str> = right.iter().map(String::as_str).collect();
    left_sorted.sort_unstable();
    right_sorted.sort_unstable();
    let mut left_index = 0;
    let mut right_index = 0;
    let mut shared = 0;
    while left_index < left_sorted.len() && right_index < right_sorted.len() {
        match left_sorted[left_index].cmp(right_sorted[right_index]) {
            std::cmp::Ordering::Less => left_index += 1,
            std::cmp::Ordering::Greater => right_index += 1,
            std::cmp::Ordering::Equal => {
                shared += 1;
                left_index += 1;
                right_index += 1;
            }
        }
    }
    shared
}

fn diff_array_myers(path: &str, left: &[Value], right: &[Value], sink: &mut ChangeSink<'_>) {
    let left_keys: Vec<String> = left.iter().map(Value::to_string).collect();
    let right_keys: Vec<String> = right.iter().map(Value::to_string).collect();
    for op in capture_diff_slices(Algorithm::Myers, &left_keys, &right_keys) {
        match op {
            DiffOp::Equal {
                old_index,
                new_index,
                len,
            } => {
                for offset in 0..len {
                    diff_value(
                        &format!("{path}[{}]", new_index + offset),
                        &left[old_index + offset],
                        &right[new_index + offset],
                        sink,
                    );
                }
            }
            DiffOp::Delete {
                old_index, old_len, ..
            } => {
                for offset in 0..old_len {
                    sink.push(StructuredChange {
                        path: format!("{path}[{}]", old_index + offset),
                        kind: StructuredChangeKind::Removed,
                        left: Some(left[old_index + offset].clone()),
                        right: None,
                        breaking: false,
                    });
                }
            }
            DiffOp::Insert {
                new_index, new_len, ..
            } => {
                for offset in 0..new_len {
                    sink.push(StructuredChange {
                        path: format!("{path}[{}]", new_index + offset),
                        kind: StructuredChangeKind::Added,
                        left: None,
                        right: Some(right[new_index + offset].clone()),
                        breaking: false,
                    });
                }
            }
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                let paired = old_len.min(new_len);
                for offset in 0..paired {
                    diff_value(
                        &format!("{path}[{}]", new_index + offset),
                        &left[old_index + offset],
                        &right[new_index + offset],
                        sink,
                    );
                }
                for offset in paired..old_len {
                    sink.push(StructuredChange {
                        path: format!("{path}[{}]", old_index + offset),
                        kind: StructuredChangeKind::Removed,
                        left: Some(left[old_index + offset].clone()),
                        right: None,
                        breaking: false,
                    });
                }
                for offset in paired..new_len {
                    sink.push(StructuredChange {
                        path: format!("{path}[{}]", new_index + offset),
                        kind: StructuredChangeKind::Added,
                        left: None,
                        right: Some(right[new_index + offset].clone()),
                        breaking: false,
                    });
                }
            }
        }
    }
}

impl ChangeSink<'_> {
    fn push(&mut self, change: StructuredChange) {
        if self.changes.len() < self.max_changes {
            self.changes.push(change);
        } else {
            *self.truncated = true;
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/diff/structured.rs"]
mod tests;
