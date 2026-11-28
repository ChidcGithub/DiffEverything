use crate::engine::ResourceBudget;
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::io::Read;
use std::sync::Arc;
use zip::ZipArchive;

/// One open ZIP package per input side. Office/OpenDocument parsing and media
/// hashing share it, so the central directory is scanned once instead of once
/// per part.
pub(crate) struct Package<'a> {
    archive: RefCell<ZipArchive<std::io::Cursor<&'a [u8]>>>,
    pub(crate) names: Vec<String>,
    /// Reused 64 KiB read buffer for streamed entry hashing.
    scratch: RefCell<Vec<u8>>,
    budget: Option<Arc<ResourceBudget>>,
    max_entry_bytes: u64,
}

impl<'a> Package<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> std::result::Result<Self, String> {
        Self::with_limits(bytes, None, u64::MAX / 2)
    }

    pub(crate) fn with_limits(
        bytes: &'a [u8],
        budget: Option<Arc<ResourceBudget>>,
        max_entry_bytes: u64,
    ) -> std::result::Result<Self, String> {
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
            budget,
            max_entry_bytes,
        })
    }

    pub(crate) fn contains(&self, name: &str) -> bool {
        self.names.iter().any(|candidate| candidate == name)
    }

    pub(crate) fn read(&self, name: &str) -> std::result::Result<Vec<u8>, String> {
        let mut archive = self.archive.borrow_mut();
        let mut entry = archive.by_name(name).map_err(|error| error.to_string())?;
        if entry.size() > self.max_entry_bytes {
            return Err(format!(
                "{name}: archive entry exceeds the detailed-diff limit"
            ));
        }
        let mut data = Vec::new();
        let mut limited = (&mut entry).take(self.max_entry_bytes.saturating_add(1));
        limited
            .read_to_end(&mut data)
            .map_err(|error| error.to_string())?;
        if data.len() as u64 > self.max_entry_bytes {
            return Err(format!(
                "{name}: archive entry exceeds the detailed-diff limit"
            ));
        }
        if let Some(budget) = &self.budget
            && !budget.charge(data.len() as u64)
        {
            return Err("expanded-bytes budget exhausted while reading document parts".into());
        }
        Ok(data)
    }

    pub(crate) fn read_text(&self, name: &str) -> std::result::Result<String, String> {
        let data = self.read(name)?;
        String::from_utf8(data).map_err(|error| error.to_string())
    }

    /// Charge an entry's declared uncompressed size without reading it. Used
    /// for parts that are handed to a third-party parser (XLSX shared strings)
    /// so the comparison budget still sees them before that parser allocates.
    pub(crate) fn charge_declared_entry(&self, name: &str) -> std::result::Result<bool, String> {
        let mut archive = self.archive.borrow_mut();
        let entry = archive.by_name(name).map_err(|error| error.to_string())?;
        let size = entry.size();
        drop(entry);
        if size > self.max_entry_bytes {
            return Err(format!(
                "{name}: archive entry exceeds the detailed-diff limit"
            ));
        }
        match &self.budget {
            Some(budget) => Ok(budget.charge(size)),
            None => Ok(true),
        }
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
        if entry.size() > self.max_entry_bytes {
            return Err(format!(
                "{name}: archive entry exceeds the detailed-diff limit"
            ));
        }
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
        if let Some(budget) = &self.budget
            && !budget.charge(size)
        {
            return Err("expanded-bytes budget exhausted while hashing document media".into());
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
