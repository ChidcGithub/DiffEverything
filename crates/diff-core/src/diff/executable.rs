use object::{Object, ObjectSection, ObjectSymbol, SymbolKind, SymbolScope};
use serde_json::{Map, Value, json};

const MAX_SECTIONS: usize = 2_000;
const MAX_SYMBOLS: usize = 5_000;
const MAX_IMPORTS: usize = 5_000;

pub fn parse_executable(bytes: &[u8]) -> Result<Value, String> {
    if let Some(result) = parse_fat_executable(bytes) {
        return result;
    }
    parse_thin_executable(bytes)
}

fn parse_fat_executable(bytes: &[u8]) -> Option<Result<Value, String>> {
    let magic = bytes.get(0..4)?;
    let is_64 = match magic {
        [0xca, 0xfe, 0xba, 0xbe] => false,
        [0xca, 0xfe, 0xba, 0xbf] => true,
        _ => return None,
    };
    Some(parse_fat_inner(bytes, is_64))
}

fn parse_fat_inner(bytes: &[u8], is_64: bool) -> Result<Value, String> {
    if bytes.len() < 8 {
        return Err("truncated fat Mach-O header".to_string());
    }
    let count = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
    if count == 0 || count > 64 {
        return Err(format!("invalid fat Mach-O architecture count: {count}"));
    }
    let entry_size = if is_64 { 32 } else { 20 };
    let table_end = 8usize
        .checked_add(count.saturating_mul(entry_size))
        .ok_or_else(|| "fat Mach-O table overflow".to_string())?;
    if table_end > bytes.len() {
        return Err("truncated fat Mach-O table".to_string());
    }

    let mut architectures = Vec::with_capacity(count);
    for index in 0..count {
        let entry = &bytes[8 + index * entry_size..8 + (index + 1) * entry_size];
        let (offset, size) = if is_64 {
            let offset = u64::from_be_bytes(entry[8..16].try_into().unwrap());
            let size = u64::from_be_bytes(entry[16..24].try_into().unwrap());
            (offset, size)
        } else {
            let offset = u64::from(u32::from_be_bytes(entry[8..12].try_into().unwrap()));
            let size = u64::from(u32::from_be_bytes(entry[12..16].try_into().unwrap()));
            (offset, size)
        };
        let offset =
            usize::try_from(offset).map_err(|_| "fat Mach-O offset overflow".to_string())?;
        let size = usize::try_from(size).map_err(|_| "fat Mach-O size overflow".to_string())?;
        let slice = offset
            .checked_add(size)
            .and_then(|end| bytes.get(offset..end))
            .ok_or_else(|| format!("fat Mach-O slice {index} is out of range"))?;
        let value = match parse_thin_executable(slice) {
            Ok(value) => value,
            Err(error) => json!({
                "error": error,
                "offset": offset,
                "size": size,
            }),
        };
        architectures.push(value);
    }

    Ok(json!({
        "format": "mach-o-fat",
        "architecture_count": count,
        "architectures": architectures,
    }))
}

fn parse_thin_executable(bytes: &[u8]) -> Result<Value, String> {
    let file = object::File::parse(bytes).map_err(|error| error.to_string())?;
    let format = format!("{:?}", file.format()).to_ascii_lowercase();

    let mut root = Map::new();
    root.insert("format".to_string(), Value::String(format));
    root.insert(
        "architecture".to_string(),
        Value::String(format!("{:?}", file.architecture())),
    );
    root.insert(
        "endianness".to_string(),
        Value::String(
            if file.is_little_endian() {
                "little"
            } else {
                "big"
            }
            .to_string(),
        ),
    );
    root.insert(
        "kind".to_string(),
        Value::String(format!("{:?}", file.kind())),
    );
    root.insert("entry".to_string(), Value::from(file.entry()));
    root.insert("is_64".to_string(), Value::Bool(file.is_64()));

    let mut sections: Vec<Value> = file
        .sections()
        .map(|section| {
            let name = section.name().unwrap_or("<invalid>").to_string();
            let data = section.data().unwrap_or_default();
            let (offset, file_size) = section.file_range().unwrap_or((0, 0));
            json!({
                "name": name,
                "address": section.address(),
                "offset": offset,
                "file_size": file_size,
                "size": section.size(),
                "kind": format!("{:?}", section.kind()),
                "digest": format!("{:016x}", fnv1a64(data)),
            })
        })
        .collect();
    sections.sort_by(|left, right| {
        left["name"]
            .as_str()
            .cmp(&right["name"].as_str())
            .then(left["address"].as_u64().cmp(&right["address"].as_u64()))
    });
    let sections_truncated = sections.len() > MAX_SECTIONS;
    sections.truncate(MAX_SECTIONS);
    root.insert("sections".to_string(), Value::Array(sections));
    root.insert(
        "sections_truncated".to_string(),
        Value::Bool(sections_truncated),
    );

    let mut symbols: Vec<Value> = file
        .symbols()
        .filter_map(|symbol| {
            let name = symbol.name().unwrap_or("").to_string();
            if name.is_empty() && symbol.address() == 0 {
                return None;
            }
            Some(json!({
                "name": name,
                "address": symbol.address(),
                "size": symbol.size(),
                "kind": symbol_kind_label(symbol.kind()),
                "scope": symbol_scope_label(symbol.scope()),
                "undefined": symbol.is_undefined(),
            }))
        })
        .collect();
    symbols.sort_by(|left, right| {
        left["name"]
            .as_str()
            .cmp(&right["name"].as_str())
            .then(left["address"].as_u64().cmp(&right["address"].as_u64()))
    });
    let symbols_truncated = symbols.len() > MAX_SYMBOLS;
    symbols.truncate(MAX_SYMBOLS);
    root.insert("symbols".to_string(), Value::Array(symbols));
    root.insert(
        "symbols_truncated".to_string(),
        Value::Bool(symbols_truncated),
    );

    let mut imports: Vec<Value> = Vec::new();
    if let Ok(iterator) = file.imports() {
        for import in iterator.flatten() {
            imports.push(json!({
                "library": String::from_utf8_lossy(import.library()),
                "name": import_name(&import),
            }));
        }
    }
    imports.sort_by(|left, right| {
        left["library"]
            .as_str()
            .cmp(&right["library"].as_str())
            .then(left["name"].as_str().cmp(&right["name"].as_str()))
    });
    imports.dedup();
    let imports_truncated = imports.len() > MAX_IMPORTS;
    imports.truncate(MAX_IMPORTS);
    root.insert("imports".to_string(), Value::Array(imports));
    root.insert(
        "imports_truncated".to_string(),
        Value::Bool(imports_truncated),
    );

    let mut libraries: Vec<String> = Vec::new();
    if let Ok(iterator) = file.import_libraries() {
        for library in iterator.flatten() {
            libraries.push(String::from_utf8_lossy(library.name()).into_owned());
        }
    }
    libraries.sort();
    libraries.dedup();
    root.insert(
        "libraries".to_string(),
        Value::Array(libraries.into_iter().map(Value::String).collect()),
    );

    Ok(Value::Object(root))
}

fn import_name(import: &object::Import<'_>) -> String {
    match import.name() {
        object::NameOrOrdinal::Name(name) => String::from_utf8_lossy(name).into_owned(),
        object::NameOrOrdinal::Ordinal(ordinal) => format!("#{ordinal}"),
    }
}

fn symbol_kind_label(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Unknown => "unknown",
        SymbolKind::Text => "text",
        SymbolKind::Data => "data",
        SymbolKind::Section => "section",
        SymbolKind::File => "file",
        SymbolKind::Label => "label",
        SymbolKind::Tls => "tls",
        _ => "unknown",
    }
}

fn symbol_scope_label(scope: SymbolScope) -> &'static str {
    match scope {
        SymbolScope::Unknown => "unknown",
        SymbolScope::Compilation => "compilation",
        SymbolScope::Linkage => "linkage",
        SymbolScope::Dynamic => "dynamic",
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
#[path = "../../tests/unit/diff/executable.rs"]
mod tests;
