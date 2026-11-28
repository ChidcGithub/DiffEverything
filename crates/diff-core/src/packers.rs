use crate::detect::detect_file;
use crate::model::FileKind;
use object::{Object, ObjectSection};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PackerInfo {
    pub name: String,
    pub evidence: String,
}

const DETECTION_READ_LIMIT: u64 = 64 * 1024 * 1024;

pub fn detect_packers(path: &Path) -> Vec<PackerInfo> {
    if !path.is_file() {
        return Vec::new();
    }
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if name.ends_with(".apk") || name.ends_with(".aab") {
        return detect_apk_packers(path);
    }
    if detect_file(path) != FileKind::Executable {
        return Vec::new();
    }
    let Ok(metadata) = fs::metadata(path) else {
        return Vec::new();
    };
    if metadata.len() > DETECTION_READ_LIMIT {
        // Section headers and typical packer signatures live near the start;
        // UPX also leaves its marker near the end. Avoid loading huge files.
        let mut data = Vec::new();
        if let Ok(mut file) = fs::File::open(path) {
            use std::io::{Read, Seek, SeekFrom};
            let head = 4 * 1024 * 1024;
            let mut buffer = vec![0u8; head];
            if let Ok(read) = file.read(&mut buffer) {
                data.extend_from_slice(&buffer[..read]);
            }
            if file.seek(SeekFrom::End(-(head as i64))).is_ok() {
                let mut buffer = vec![0u8; head];
                if let Ok(read) = file.read(&mut buffer) {
                    data.extend_from_slice(&buffer[..read]);
                }
            }
        }
        return detect_packers_bytes(&data);
    }
    fs::read(path)
        .map(|bytes| detect_packers_bytes(&bytes))
        .unwrap_or_default()
}

pub fn detect_packers_bytes(bytes: &[u8]) -> Vec<PackerInfo> {
    let mut found = BTreeMap::new();

    if let Ok(file) = object::File::parse(bytes) {
        for section in file.sections() {
            let Ok(name) = section.name() else {
                continue;
            };
            if let Some(packer) = packer_for_section(name) {
                found
                    .entry(packer.to_string())
                    .or_insert_with(|| format!("section {name}"));
            }
        }
    }

    found
        .into_iter()
        .map(|(name, evidence)| PackerInfo { name, evidence })
        .collect()
}

fn detect_apk_packers(path: &Path) -> Vec<PackerInfo> {
    let Ok(file) = fs::File::open(path) else {
        return Vec::new();
    };
    let Ok(mut archive) = zip::ZipArchive::new(file) else {
        return Vec::new();
    };
    let mut found = BTreeMap::new();
    for index in 0..archive.len() {
        let Ok(entry) = archive.by_index(index) else {
            continue;
        };
        let name = entry.name().replace('\\', "/").to_ascii_lowercase();
        if let Some(packer) = packer_for_apk_entry(&name) {
            found
                .entry(packer.to_string())
                .or_insert_with(|| format!("entry {name}"));
        }
    }
    found
        .into_iter()
        .map(|(name, evidence)| PackerInfo { name, evidence })
        .collect()
}

fn packer_for_section(name: &str) -> Option<&'static str> {
    let name = name.to_ascii_lowercase();
    match name.as_str() {
        "upx0" | "upx1" | "upx2" | ".upx" => Some("UPX"),
        ".aspack" => Some("ASPack"),
        ".themida" => Some("Themida"),
        ".vmp0" | ".vmp1" => Some("VMProtect"),
        ".enigma1" | ".enigma2" | ".enigma" => Some("Enigma Protector"),
        ".mpress1" | ".mpress2" => Some("MPRESS"),
        ".pec" | "pecompact2" => Some("PECompact"),
        ".petite" => Some("Petite"),
        ".nsp0" | ".nsp1" | ".nsp2" => Some("NSPack"),
        ".armadillo" => Some("Armadillo"),
        ".obsidium" => Some("Obsidium"),
        ".pelock" => Some("PELock"),
        ".telock" => Some("tElock"),
        ".molebox" => Some("MoleBox"),
        ".boxed" => Some("BoxedApp"),
        ".rlpack" => Some("RLPack"),
        _ => None,
    }
}

fn packer_for_apk_entry(name: &str) -> Option<&'static str> {
    if name.contains("libjiagu") {
        return Some("360 Jiagu");
    }
    if name.contains("libshella") || name.contains("libshellx") {
        return Some("Tencent Legu");
    }
    if name.contains("libsecexe") || name.contains("libsecmain") || name.contains("libdexhelper") {
        return Some("Bangcle");
    }
    if name.contains("libmobisec") {
        return Some("Alibaba Mobile Security");
    }
    if name.contains("libbaiduprotect") {
        return Some("Baidu Protector");
    }
    if name.contains("libnqshield") {
        return Some("Naqu Shield");
    }
    if name.contains("libnesec") {
        return Some("NetEase Shield");
    }
    None
}

#[cfg(test)]
#[path = "../tests/unit/packers.rs"]
mod tests;
