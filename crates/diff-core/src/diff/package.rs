use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::io::Read;
use zip::ZipArchive;

/// One open ZIP package per input side. Office/OpenDocument parsing and media
/// hashing share it, so the central directory is scanned once instead of once
/// per part.
pub(crate) struct Package<'a> {
    archive: RefCell<ZipArchive<std::io::Cursor<&'a [u8]>>>,
    pub(crate) names: Vec<String>,
    /// Reused 64 KiB read buffer for streamed entry hashing.
    scratch: RefCell<Vec<u8>>,
}

impl<'a> Package<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> std::result::Result<Self, String> {
        let mut archive =
            ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|error| error.to_string())?;
        let mut names = Vec::with_capacity(archive.len());
        for index in 0..archive.len() {
            let entry = archive.by_index(index).map_err(|error| error.to_string())?;
            names.push(entry.name().to_string());
        }
        Ok(Self {
            archive: RefCell::new(archive),
            names,
            scratch: RefCell::new(vec![0; 64 * 1024]),
        })
    }

    pub(crate) fn contains(&self, name: &str) -> bool {
        self.names.iter().any(|candidate| candidate == name)
    }

    pub(crate) fn read(&self, name: &str) -> std::result::Result<Vec<u8>, String> {
        let mut archive = self.archive.borrow_mut();
        let mut entry = archive.by_name(name).map_err(|error| error.to_string())?;
        let mut data = Vec::new();
        entry
            .read_to_end(&mut data)
            .map_err(|error| error.to_string())?;
        Ok(data)
    }

    pub(crate) fn read_text(&self, name: &str) -> std::result::Result<String, String> {
        let data = self.read(name)?;
        String::from_utf8(data).map_err(|error| error.to_string())
    }

    /// Hash one entry without materialising it. The same 64 KiB buffer is
    /// reused for every entry and every comparison side that owns a package.
    pub(crate) fn hash_entry(
        &self,
        name: &str,
    ) -> std::result::Result<Option<(String, u64)>, String> {
        let mut archive = self.archive.borrow_mut();
        let mut entry = match archive.by_name(name) {
            Ok(entry) => entry,
            Err(_) => return Ok(None),
        };
        let mut scratch = self.scratch.borrow_mut();
        let mut hasher = Sha256::new();
        let mut size = 0u64;
        loop {
            let read = entry
                .read(&mut scratch)
                .map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            hasher.update(&scratch[..read]);
            size += read as u64;
        }
        let digest = hasher.finalize();
        let mut hash = String::with_capacity(digest.len() * 2);
        for byte in digest {
            use std::fmt::Write as _;
            let _ = write!(hash, "{byte:02x}");
        }
        Ok(Some((hash, size)))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/diff/package.rs"]
mod tests;
