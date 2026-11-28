use crate::error::{DiffError, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrOptions {
    pub tesseract: PathBuf,
    pub tessdata: Option<PathBuf>,
    pub language: String,
    /// Optional poppler renderer for scanned PDFs.
    pub pdftoppm: Option<PathBuf>,
    pub dpi: u32,
    pub max_pages: usize,
}

impl OcrOptions {
    pub fn image(tesseract: PathBuf, tessdata: Option<PathBuf>, language: String) -> Self {
        Self {
            tesseract,
            tessdata,
            language,
            pdftoppm: None,
            dpi: 100,
            max_pages: 50,
        }
    }
}

pub fn ocr_image_bytes(bytes: &[u8], options: &OcrOptions) -> Result<String> {
    if !options.tesseract.is_file() {
        return Err(DiffError::Invalid(format!(
            "tesseract not found at {}",
            options.tesseract.display()
        )));
    }

    let mut command = Command::new(&options.tesseract);
    command
        .arg("stdin")
        .arg("stdout")
        .arg("-l")
        .arg(&options.language)
        .arg("--psm")
        .arg("6")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(tessdata) = &options.tessdata {
        command.env("TESSDATA_PREFIX", tessdata);
    } else if let Some(parent) = options.tesseract.parent() {
        command.current_dir(parent);
    }

    let mut child = command
        .spawn()
        .map_err(|error| DiffError::Invalid(format!("cannot run tesseract: {error}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(bytes).map_err(|error| {
            DiffError::Invalid(format!("cannot write image to tesseract: {error}"))
        })?;
    }
    let output = child
        .wait_with_output()
        .map_err(|error| DiffError::Invalid(format!("tesseract failed: {error}")))?;
    if !output.status.success() {
        return Err(DiffError::Invalid(format!(
            "tesseract failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Render a PDF with poppler and OCR every page. This is the fallback for
/// scanned PDFs that have no text layer.
pub fn ocr_pdf_text(pdf: &Path, options: &OcrOptions) -> Result<String> {
    let pdftoppm = options.pdftoppm.as_ref().ok_or_else(|| {
        DiffError::Invalid(
            "PDF OCR requires pdftoppm; install poppler or pass --pdftoppm".to_string(),
        )
    })?;
    let temp_dir = temp_dir_path("diff-everything-ocr");
    let result = (|| -> Result<String> {
        let pages = crate::pdf_visual::render_pdf_pages(
            pdf,
            &temp_dir,
            pdftoppm,
            options.dpi,
            options.max_pages,
        )?;
        if pages.is_empty() {
            return Err(DiffError::Invalid(format!(
                "no pages rendered from {}",
                pdf.display()
            )));
        }
        let mut out = String::new();
        for (index, page) in pages.iter().enumerate() {
            let bytes = std::fs::read(page)?;
            let text = ocr_image_bytes(&bytes, options)?;
            if !text.trim().is_empty() {
                out.push_str(&format!("# Page {}\n{}\n\n", index + 1, text.trim()));
            }
        }
        Ok(out.trim_end().to_string())
    })();
    let _ = std::fs::remove_dir_all(&temp_dir);
    result
}

fn temp_dir_path(prefix: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("{prefix}-{}-{stamp}", std::process::id()))
}

#[cfg(test)]
#[path = "../tests/unit/ocr.rs"]
mod tests;
