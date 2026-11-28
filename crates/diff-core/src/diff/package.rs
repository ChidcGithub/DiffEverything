use std::cell::RefCell;
use std::io::Read;
use zip::ZipArchive;

/// One open ZIP package per input side. Office/OpenDocument parsing and media
/// hashing share it, so the central directory is scanned once instead of once
/// per part.
pub(crate) struct Package<'a> {
    archive: RefCell<ZipArchive<std::io::Cursor<&'a [u8]>>>,
    pub(crate) names: Vec<String>,
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
}
