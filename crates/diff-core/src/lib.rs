#![forbid(unsafe_code)]

pub mod concise;
pub mod detect;
pub mod diff;
pub mod disassembly;
pub mod engine;
pub mod error;
pub mod filter;
pub mod folder_merge;
pub mod folder_patch;
pub mod image_export;
pub mod merge;
pub mod model;
pub mod ocr;
pub mod patch;
pub mod pdf_visual;
pub mod progress;
pub mod render;
pub mod security;
pub mod session;
pub mod sync;

pub use engine::{DiffEngine, DiffOptions};
pub use error::{DiffError, Result};
pub use model::{DiffReport, DiffStatus, FileKind};
