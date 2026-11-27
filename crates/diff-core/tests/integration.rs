//! Single integration-test target for diff-core.
//!
//! The individual case files live under `tests/cases/` and are compiled as
//! modules of this one test binary. Keeping one link step instead of six is a
//! large win on Windows, where each test executable links the full dependency
//! graph.

#[path = "cases/engine.rs"]
mod engine;

#[path = "cases/extended_formats.rs"]
mod extended_formats;

#[path = "cases/folder_merge.rs"]
mod folder_merge;

#[path = "cases/folder_patch.rs"]
mod folder_patch;

#[path = "cases/formats.rs"]
mod formats;

#[path = "cases/robustness.rs"]
mod robustness;
