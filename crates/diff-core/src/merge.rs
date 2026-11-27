use crate::diff::text::decode_text;
use crate::error::Result;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct MergeResult {
    pub text: String,
    pub clean: bool,
}

pub fn merge_text(base: &[u8], left: &[u8], right: &[u8]) -> MergeResult {
    let base = decode_text(base).text;
    let left = decode_text(left).text;
    let right = decode_text(right).text;
    match diffy::merge(&base, &left, &right) {
        Ok(text) => MergeResult { text, clean: true },
        Err(text) => MergeResult { text, clean: false },
    }
}

pub fn merge_files(base: &Path, left: &Path, right: &Path) -> Result<MergeResult> {
    let base_bytes = std::fs::read(base)?;
    let left_bytes = std::fs::read(left)?;
    let right_bytes = std::fs::read(right)?;
    if base
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("ipynb"))
    {
        let merged = crate::diff::notebook::merge_notebooks(&base_bytes, &left_bytes, &right_bytes)
            .map_err(crate::error::DiffError::Invalid)?;
        return Ok(MergeResult {
            text: merged.text,
            clean: merged.clean,
        });
    }
    Ok(merge_text(&base_bytes, &left_bytes, &right_bytes))
}
