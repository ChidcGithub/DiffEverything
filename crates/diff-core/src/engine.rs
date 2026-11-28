use crate::detect::{detect_bytes, detect_file};
use crate::diff::{
    binary, documents, executable, font, image as image_diff, sqlite, structured, table, text,
    tokens, tree_diff, workbook,
};
use crate::error::{DiffError, Result};
use crate::model::*;
use crate::progress::ProgressHandle;
use globset::{Glob, GlobSet, GlobSetBuilder};
use quick_xml::XmlVersion;
use quick_xml::events::Event;
use rayon::ThreadPoolBuilder;
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use zip::ZipArchive;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffMode {
    #[default]
    Text,
    Token,
    Tree,
    Graph,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DecompileOptions {
    #[serde(default)]
    pub objdump: Option<PathBuf>,
    #[serde(default = "default_decompile_max_lines")]
    pub max_lines: usize,
}

fn default_decompile_max_lines() -> usize {
    20_000
}

impl Default for DecompileOptions {
    fn default() -> Self {
        Self {
            objdump: None,
            max_lines: default_decompile_max_lines(),
        }
    }
}

/// Sentinel used when the caller explicitly disables the per-file size limit.
/// It stays well below `u64::MAX` so internal `limit + 1` reads cannot overflow.
pub const UNLIMITED_FILE_BYTES: u64 = u64::MAX / 2;

/// Default budget for the total amount of unpacked data (archive entries and
/// decompressed streams, both sides, every nesting level) one comparison may
/// produce. Per-entry limits bound one payload; this bounds their sum.
pub const DEFAULT_MAX_EXPANDED_BYTES: u64 = 1024 * 1024 * 1024;

/// Default budget for the bytes of file contents the parallel workers of one
/// comparison may hold in flight at the same time.
pub const DEFAULT_MAX_INFLIGHT_BYTES: u64 = 256 * 1024 * 1024;

/// Bounds the bytes of file contents the workers of one comparison may hold in
/// flight at the same time. Each task acquires its footprint (left + right
/// file size) before its contents are read and releases it when the node is
/// built, so the parallel work set stays bounded regardless of `jobs`: more
/// workers means more small tasks in flight, never more bytes.
///
/// A task whose footprint alone exceeds the budget is clamped to it, so it
/// always runs instead of deadlocking. `max_bytes == 0` disables the gate.
#[derive(Debug)]
struct InflightGate {
    max_bytes: u64,
    used: std::sync::Mutex<u64>,
    available: std::sync::Condvar,
}

impl InflightGate {
    fn new(max_bytes: u64) -> Self {
        Self {
            max_bytes,
            used: std::sync::Mutex::new(0),
            available: std::sync::Condvar::new(),
        }
    }

    fn acquire(&self, bytes: u64) -> InflightPermit<'_> {
        let footprint = if self.max_bytes == 0 {
            0
        } else {
            bytes.min(self.max_bytes)
        };
        if footprint == 0 {
            return InflightPermit {
                gate: self,
                footprint: 0,
            };
        }
        let mut used = self.used.lock().expect("in-flight gate poisoned");
        while *used + footprint > self.max_bytes {
            used = self.available.wait(used).expect("in-flight gate poisoned");
        }
        *used += footprint;
        InflightPermit {
            gate: self,
            footprint,
        }
    }

    fn release(&self, footprint: u64) {
        if footprint == 0 {
            return;
        }
        let mut used = self.used.lock().expect("in-flight gate poisoned");
        *used -= footprint;
        self.available.notify_all();
    }
}

/// Held while a task's file contents are in memory; releases the footprint on
/// drop, waking the workers that are waiting for room.
struct InflightPermit<'a> {
    gate: &'a InflightGate,
    footprint: u64,
}

impl Drop for InflightPermit<'_> {
    fn drop(&mut self) {
        self.gate.release(self.footprint);
    }
}

#[derive(Debug, Clone, Copy)]
struct ZipEntryMeta {
    index: usize,
    size: u64,
    compressed_size: u64,
    is_lzma: bool,
}

#[derive(Debug, Clone)]
struct EntryStamp {
    hash: String,
    size: u64,
    kind: FileKind,
}

/// Streaming ZIP reader used by the archive comparison. The central directory
/// is scanned once, but entry payloads are decompressed on demand and dropped
/// immediately after use instead of keeping both sides' contents resident.
struct ZipStream<'a> {
    archive: Mutex<ZipArchive<std::io::Cursor<&'a [u8]>>>,
    entries: BTreeMap<String, ZipEntryMeta>,
    charged: Mutex<BTreeSet<String>>,
    stamps: Mutex<BTreeMap<String, EntryStamp>>,
    stopped: AtomicBool,
    max_file_bytes: u64,
    label: String,
}

trait StreamEntries {
    fn names(&self) -> Vec<&str>;
    fn contains(&self, name: &str) -> bool;
    fn size(&self, name: &str) -> Option<u64>;
    fn read(&self, name: &str, ctx: &mut Ctx) -> Result<Option<Vec<u8>>>;
    fn stamp(&self, name: &str, ctx: &mut Ctx) -> Result<Option<EntryStamp>>;
}

impl StreamEntries for ZipStream<'_> {
    fn names(&self) -> Vec<&str> {
        self.entries.keys().map(String::as_str).collect()
    }

    fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    fn size(&self, name: &str) -> Option<u64> {
        self.entries.get(name).map(|meta| meta.size)
    }

    fn read(&self, name: &str, ctx: &mut Ctx) -> Result<Option<Vec<u8>>> {
        ZipStream::read(self, name, ctx)
    }

    fn stamp(&self, name: &str, ctx: &mut Ctx) -> Result<Option<EntryStamp>> {
        ZipStream::stamp(self, name, ctx)
    }
}

impl ZipStream<'_> {
    fn read(&self, name: &str, ctx: &mut Ctx) -> Result<Option<Vec<u8>>> {
        if self.stopped.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let Some(meta) = self.entries.get(name).copied() else {
            return Ok(None);
        };
        let mut archive = self.archive.lock().expect("zip stream poisoned");
        let data = if meta.is_lzma {
            if meta.compressed_size > self.max_file_bytes {
                drop(archive);
                ctx.warn(format!(
                    "{}: archive entry {name} has more compressed bytes than the detailed-diff limit",
                    display_label(&self.label)
                ));
                ctx.omit();
                return Ok(None);
            }
            let mut raw = archive
                .by_index_raw(meta.index)
                .map_err(|error| DiffError::Invalid(format!("zip lzma entry #{name}: {error}")))?;
            let mut compressed = Vec::with_capacity(reserve_for_declared(meta.compressed_size));
            read_bounded(&mut raw, self.max_file_bytes, &mut compressed)
                .map_err(|error| DiffError::Invalid(format!("zip lzma entry {name}: {error}")))?;
            if compressed.len() as u64 > self.max_file_bytes {
                ctx.warn(format!(
                    "{}: archive entry {name} exceeds the detailed-diff limit",
                    display_label(&self.label)
                ));
                ctx.omit();
                return Ok(None);
            }
            decode_zip_lzma(&compressed, meta.size, self.max_file_bytes)
                .map_err(|error| DiffError::Invalid(format!("zip lzma entry {name}: {error}")))?
        } else {
            let mut entry = archive
                .by_index(meta.index)
                .map_err(|error| DiffError::Invalid(format!("zip entry {name}: {error}")))?;
            let mut data = Vec::new();
            let mut limited = (&mut entry).take(self.max_file_bytes + 1);
            limited
                .read_to_end(&mut data)
                .map_err(|error| DiffError::Invalid(format!("zip entry {name}: {error}")))?;
            if data.len() as u64 > self.max_file_bytes {
                ctx.warn(format!(
                    "{}: archive entry {name} exceeds the detailed-diff limit",
                    display_label(&self.label)
                ));
                ctx.omit();
                return Ok(None);
            }
            data
        };
        drop(archive);

        let already_charged = {
            let charged = self.charged.lock().expect("zip charge set poisoned");
            charged.contains(name)
        };
        if !already_charged {
            if !ctx.charge_expanded(&self.label, data.len()) {
                self.stopped.store(true, Ordering::Relaxed);
                return Ok(None);
            }
            self.charged
                .lock()
                .expect("zip charge set poisoned")
                .insert(name.to_string());
        }
        Ok(Some(data))
    }

    fn stamp(&self, name: &str, ctx: &mut Ctx) -> Result<Option<EntryStamp>> {
        if let Some(stamp) = self
            .stamps
            .lock()
            .expect("zip stamp map poisoned")
            .get(name)
            .cloned()
        {
            return Ok(Some(stamp));
        }
        let Some(data) = self.read(name, ctx)? else {
            return Ok(None);
        };
        let stamp = EntryStamp {
            hash: hex_bytes(&Sha256::digest(&data)),
            size: data.len() as u64,
            kind: detect_bytes(name, &data),
        };
        self.stamps
            .lock()
            .expect("zip stamp map poisoned")
            .insert(name.to_string(), stamp.clone());
        Ok(Some(stamp))
    }
}

#[derive(Debug, Clone)]
enum SliceData {
    Range(usize, usize),
    Owned(Vec<u8>),
}

#[derive(Debug, Clone)]
struct SliceEntryMeta {
    data: SliceData,
    size: u64,
}

/// Streaming reader for uncompressed TAR and other seekable archives. The
/// catalog scan records each payload's byte range; payload bytes are sliced on
/// demand instead of being copied into a map for both sides. Compressed TAR
/// streams keep the materialising reader because seeking inside a compressed
/// stream is not generally possible.
struct SliceStream<'a> {
    bytes: &'a [u8],
    entries: BTreeMap<String, SliceEntryMeta>,
    charged: Mutex<BTreeSet<String>>,
    stamps: Mutex<BTreeMap<String, EntryStamp>>,
    stopped: AtomicBool,
    max_file_bytes: u64,
    label: String,
}

impl SliceStream<'_> {
    fn read(&self, name: &str, ctx: &mut Ctx) -> Result<Option<Vec<u8>>> {
        if self.stopped.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let Some(meta) = self.entries.get(name) else {
            return Ok(None);
        };
        let data = match &meta.data {
            SliceData::Owned(data) => data.clone(),
            SliceData::Range(offset, size) => {
                let end = offset.checked_add(*size).ok_or_else(|| {
                    DiffError::Invalid(format!("archive entry {name}: payload offset overflow"))
                })?;
                let Some(data) = self.bytes.get(*offset..end) else {
                    return Err(DiffError::Invalid(format!(
                        "archive entry {name}: payload exceeds archive size"
                    )));
                };
                data.to_vec()
            }
        };
        if data.len() as u64 > self.max_file_bytes {
            ctx.warn(format!(
                "{}: archive entry {name} exceeds the detailed-diff limit",
                display_label(&self.label)
            ));
            ctx.omit();
            return Ok(None);
        }
        let already_charged = {
            let charged = self.charged.lock().expect("tar charge set poisoned");
            charged.contains(name)
        };
        if !already_charged {
            if !ctx.charge_expanded(&self.label, data.len()) {
                self.stopped.store(true, Ordering::Relaxed);
                return Ok(None);
            }
            self.charged
                .lock()
                .expect("tar charge set poisoned")
                .insert(name.to_string());
        }
        Ok(Some(data))
    }

    fn stamp(&self, name: &str, ctx: &mut Ctx) -> Result<Option<EntryStamp>> {
        if let Some(stamp) = self
            .stamps
            .lock()
            .expect("tar stamp map poisoned")
            .get(name)
            .cloned()
        {
            return Ok(Some(stamp));
        }
        let Some(data) = self.read(name, ctx)? else {
            return Ok(None);
        };
        let stamp = EntryStamp {
            hash: hex_bytes(&Sha256::digest(&data)),
            size: data.len() as u64,
            kind: detect_bytes(name, &data),
        };
        self.stamps
            .lock()
            .expect("tar stamp map poisoned")
            .insert(name.to_string(), stamp.clone());
        Ok(Some(stamp))
    }
}

impl StreamEntries for SliceStream<'_> {
    fn names(&self) -> Vec<&str> {
        self.entries.keys().map(String::as_str).collect()
    }

    fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    fn size(&self, name: &str) -> Option<u64> {
        self.entries.get(name).map(|meta| meta.size)
    }

    fn read(&self, name: &str, ctx: &mut Ctx) -> Result<Option<Vec<u8>>> {
        SliceStream::read(self, name, ctx)
    }

    fn stamp(&self, name: &str, ctx: &mut Ctx) -> Result<Option<EntryStamp>> {
        SliceStream::stamp(self, name, ctx)
    }
}

/// Shared budget for the total unpacked bytes a comparison may produce.
///
/// Every archive reader and decompressor charges the bytes it materializes.
/// Charging is atomic so rayon workers cannot race past the cap; a worker may
/// overshoot by at most one entry (it charges after reading), which the
/// per-entry limit already bounds. Once the budget is spent it stays spent.
#[derive(Debug)]
pub struct ResourceBudget {
    max_expanded_bytes: u64,
    expanded_bytes: AtomicU64,
    exhausted: AtomicBool,
}

impl Default for ResourceBudget {
    fn default() -> Self {
        Self::new(UNLIMITED_FILE_BYTES)
    }
}

impl ResourceBudget {
    pub fn new(max_expanded_bytes: u64) -> Self {
        Self {
            max_expanded_bytes,
            expanded_bytes: AtomicU64::new(0),
            exhausted: AtomicBool::new(false),
        }
    }

    pub fn max_expanded_bytes(&self) -> u64 {
        self.max_expanded_bytes
    }

    pub fn expanded_bytes(&self) -> u64 {
        self.expanded_bytes.load(Ordering::Relaxed)
    }

    pub fn is_exhausted(&self) -> bool {
        self.exhausted.load(Ordering::Relaxed)
    }

    /// Charges `bytes` of unpacked data; `false` means the budget is spent and
    /// the caller must stop materializing more data.
    ///
    /// The newest toolchains deprecate `fetch_update` in favour of a renamed
    /// `try_update`, which does not exist on the versions this crate compiles
    /// against, so the deprecation lint is silenced here.
    #[allow(deprecated)]
    pub fn charge(&self, bytes: u64) -> bool {
        if self.is_exhausted() {
            return false;
        }
        self.expanded_bytes
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current
                    .checked_add(bytes)
                    .filter(|total| *total <= self.max_expanded_bytes)
            })
            .is_ok()
    }

    /// Latches exhaustion and returns `true` only for the caller that first
    /// observed it, so the warning is emitted once across all workers.
    pub fn announce_exhaustion(&self) -> bool {
        !self.exhausted.swap(true, Ordering::Relaxed)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiffOptions {
    pub context: usize,
    pub max_hunks: usize,
    /// Display-only concise cap. It limits how much detail readers build and
    /// keep, but never changes comparison completeness or exit codes.
    #[serde(default)]
    pub concise_limit: Option<usize>,
    pub max_rows: usize,
    pub max_file_bytes: u64,
    pub max_dir_entries: usize,
    pub max_zip_entries: usize,
    pub max_json_changes: usize,
    pub max_table_rows: usize,
    pub max_workbook_cells: usize,
    #[serde(default = "default_max_doc_blocks")]
    pub max_doc_blocks: usize,
    #[serde(default = "default_max_slides")]
    pub max_slides: usize,
    pub max_depth: usize,
    pub key_column: Option<String>,
    #[serde(default)]
    pub ignore_columns: Vec<String>,
    #[serde(default)]
    pub numeric_tolerance: f64,
    #[serde(default)]
    pub compare_formulas: bool,
    pub excludes: Vec<String>,
    pub mode: DiffMode,
    pub ignore_comments: bool,
    pub ignore_whitespace: bool,
    pub ignore_case: bool,
    #[serde(default)]
    pub image_threshold: f64,
    pub ocr: Option<crate::ocr::OcrOptions>,
    #[serde(default)]
    pub pdf_visual: Option<crate::pdf_visual::PdfVisualOptions>,
    #[serde(default)]
    pub pdf_visual_output: Option<PathBuf>,
    #[serde(default)]
    pub decompile: Option<DecompileOptions>,
    /// Worker threads for directory/archive entry comparison. 0 uses the
    /// rayon global pool; 1 disables parallelism; default 4.
    #[serde(default = "default_jobs")]
    pub jobs: usize,
    /// Total unpacked bytes (archive entries, decompressed streams, every
    /// nesting level, both sides) one comparison may materialize.
    #[serde(default = "default_max_expanded_bytes")]
    pub max_expanded_bytes: u64,
    /// Bytes of file contents the parallel workers may hold in flight at the
    /// same time; `0` disables the bound.
    #[serde(default = "default_max_inflight_bytes")]
    pub max_inflight_bytes: u64,
}

fn default_jobs() -> usize {
    4
}

fn default_max_expanded_bytes() -> u64 {
    DEFAULT_MAX_EXPANDED_BYTES
}

fn default_max_inflight_bytes() -> u64 {
    DEFAULT_MAX_INFLIGHT_BYTES
}

fn default_max_doc_blocks() -> usize {
    10_000
}

fn default_max_slides() -> usize {
    2_000
}

impl Default for DiffOptions {
    fn default() -> Self {
        Self {
            context: 3,
            max_hunks: 200,
            concise_limit: None,
            max_rows: 200,
            max_file_bytes: 64 * 1024 * 1024,
            max_dir_entries: 10_000,
            max_zip_entries: 2_000,
            max_json_changes: 500,
            max_table_rows: 500,
            max_workbook_cells: 5_000,
            max_doc_blocks: 10_000,
            max_slides: 2_000,
            max_depth: 32,
            key_column: None,
            ignore_columns: Vec::new(),
            numeric_tolerance: 0.0,
            compare_formulas: false,
            excludes: Vec::new(),
            mode: DiffMode::Text,
            ignore_comments: false,
            ignore_whitespace: false,
            ignore_case: false,
            image_threshold: 0.0,
            ocr: None,
            pdf_visual: None,
            pdf_visual_output: None,
            decompile: None,
            jobs: 4,
            max_expanded_bytes: DEFAULT_MAX_EXPANDED_BYTES,
            max_inflight_bytes: DEFAULT_MAX_INFLIGHT_BYTES,
        }
    }
}

pub struct DiffEngine {
    options: DiffOptions,
    progress: Option<ProgressHandle>,
    gate: InflightGate,
}

impl DiffEngine {
    pub fn new(options: DiffOptions) -> Self {
        let gate = InflightGate::new(options.max_inflight_bytes);
        Self {
            gate,
            options,
            progress: None,
        }
    }

    pub fn with_progress(mut self, progress: ProgressHandle) -> Self {
        self.progress = Some(progress);
        self
    }

    pub fn options(&self) -> &DiffOptions {
        &self.options
    }

    pub fn compare(&self, left: &Path, right: &Path) -> Result<DiffReport> {
        let started = Instant::now();
        let left_kind = detect_file(left);
        let right_kind = detect_file(right);
        let mut ctx = Ctx::default();
        ctx.budget = Arc::new(ResourceBudget::new(self.options.max_expanded_bytes));
        ctx.excludes = self.build_excludes(&mut ctx).map(Arc::new);
        let pool = if self.options.jobs > 1 {
            Some(
                ThreadPoolBuilder::new()
                    .num_threads(self.options.jobs)
                    .build()
                    .map_err(|error| DiffError::Invalid(format!("worker pool: {error}")))?,
            )
        } else {
            None
        };
        if let Some(progress) = &self.progress {
            progress.start("Comparing");
        }
        let result = if let Some(pool) = &pool {
            pool.install(|| self.compare_paths(left, right, "", 0, &mut ctx))
        } else {
            self.compare_paths(left, right, "", 0, &mut ctx)
        };
        let root = match result {
            Ok(root) => root,
            Err(error) => {
                if let Some(progress) = &self.progress {
                    progress.finish();
                }
                return Err(error);
            }
        };
        let completely_different = is_completely_different(&root);
        let completeness = root.completeness_of();
        let mut report = DiffReport {
            tool: TOOL_NAME,
            version: TOOL_VERSION,
            left: ArtifactInfo::from_path(left, left_kind),
            right: ArtifactInfo::from_path(right, right_kind),
            status: root.status,
            summary: ctx.summary,
            root,
            warnings: ctx.warnings,
            elapsed_ms: started.elapsed().as_millis(),
            completely_different,
            completeness,
            expanded_bytes: ctx.budget.expanded_bytes(),
            decompiled: None,
        };
        if let Some(options) = &self.options.decompile
            && left_kind == FileKind::Executable
            && right_kind == FileKind::Executable
        {
            if let Some(progress) = &self.progress {
                progress.set_message("Disassembling code");
            }
            let mut disassembly = crate::disassembly::DisassemblyOptions::new(
                options.objdump.clone(),
                self.options.context,
                self.options.max_hunks,
            );
            disassembly.max_lines = options.max_lines;
            match crate::disassembly::compare_paths(left, right, &disassembly) {
                Ok(decompiled) => report.decompiled = Some(decompiled),
                Err(error) => report
                    .warnings
                    .push(format!("decompilation failed: {error}")),
            }
        }
        if let Some(progress) = &self.progress {
            progress.finish();
        }
        Ok(report)
    }

    fn compare_paths(
        &self,
        left: &Path,
        right: &Path,
        label: &str,
        depth: usize,
        ctx: &mut Ctx,
    ) -> Result<DiffNode> {
        if depth > self.options.max_depth {
            return Ok(depth_limit_node(label, "maximum recursion depth reached"));
        }

        let left_meta = fs::metadata(left)?;
        let right_meta = fs::metadata(right)?;

        if left_meta.is_dir() && right_meta.is_dir() {
            if !can_expand(depth, self.options.max_depth) {
                return Ok(depth_limit_node(label, "directory depth limit reached"));
            }
            return self.compare_directories(left, right, label, depth, ctx);
        }

        if left_meta.is_dir() || right_meta.is_dir() {
            let note = if left_meta.is_dir() {
                "left side is a directory, right side is a file"
            } else {
                "left side is a file, right side is a directory"
            };
            ctx.warn(format!("{label}: {note}"));
            return Ok(DiffNode::new(
                label,
                DiffStatus::Different,
                DiffBody::Unsupported(UnsupportedInfo {
                    note: note.to_string(),
                }),
            ));
        }

        self.compare_files(left, right, label, depth, ctx)
    }

    fn compare_files(
        &self,
        left: &Path,
        right: &Path,
        label: &str,
        depth: usize,
        ctx: &mut Ctx,
    ) -> Result<DiffNode> {
        let left_kind = detect_file(left);
        let right_kind = detect_file(right);
        let kind = effective_kind(left_kind, right_kind);

        if left_kind != right_kind && !(left_kind.is_textual() && right_kind.is_textual()) {
            ctx.warn(format!(
                "{}: type mismatch ({} vs {}), comparing as {}",
                display_label(label),
                left_kind.label(),
                right_kind.label(),
                kind.label()
            ));
        }

        let left_meta = fs::metadata(left)?;
        let right_meta = fs::metadata(right)?;
        let hint = left
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();

        if left_kind == FileKind::Pdf
            && right_kind == FileKind::Pdf
            && let Some(node) = self.compare_pdf_ocr(left, right, label, ctx)
        {
            ctx.summary.files_compared += 1;
            if node.status == DiffStatus::Equal {
                ctx.summary.files_equal += 1;
            } else {
                ctx.summary.files_changed += 1;
            }
            return Ok(node);
        }

        if left_kind == FileKind::Video
            && right_kind == FileKind::Video
            && let Some(node) = self.compare_video_files(left, right, label, ctx)
        {
            ctx.summary.files_compared += 1;
            if node.status == DiffStatus::Equal {
                ctx.summary.files_equal += 1;
            } else {
                ctx.summary.files_changed += 1;
            }
            return Ok(node);
        }

        if left_kind == FileKind::Audio
            && right_kind == FileKind::Audio
            && let Some(node) = self.compare_audio_files(left, right, label, ctx)
        {
            ctx.summary.files_compared += 1;
            if node.status == DiffStatus::Equal {
                ctx.summary.files_equal += 1;
            } else {
                ctx.summary.files_changed += 1;
            }
            return Ok(node);
        }

        let limit = self.options.max_file_bytes;
        if left_meta.len() > limit || right_meta.len() > limit {
            return self.stream_summary_node(
                label,
                left,
                right,
                left_meta.len(),
                right_meta.len(),
                kind,
                ctx,
            );
        }

        // The file may grow between the metadata check above and this read, so
        // the read itself is bounded (TOCTOU) and re-checked against the limit.
        // Bounded scheduler: acquire the in-flight footprint (left + right
        // file size) before the contents are materialized and release it when
        // the node is built, so the parallel work set stays inside
        // `max_inflight_bytes` no matter how many workers there are.
        let permit = self
            .gate
            .acquire(left_meta.len().saturating_add(right_meta.len()));
        let left_bytes = read_file_bounded(left, limit)?;
        let right_bytes = read_file_bounded(right, limit)?;
        if left_bytes.len() as u64 > limit || right_bytes.len() as u64 > limit {
            drop(permit);
            return self.stream_summary_node(
                label,
                left,
                right,
                left_bytes.len() as u64,
                right_bytes.len() as u64,
                kind,
                ctx,
            );
        }

        ctx.summary.files_compared += 1;

        if left_bytes == right_bytes && self.options.pdf_visual.is_none() {
            ctx.summary.files_equal += 1;
            return Ok(equal_node(label, kind, "files are byte-identical"));
        }

        // Containers keep their input buffer alive while expanding nested
        // entries. Holding the in-flight permit across that recursion can
        // deadlock the gate: children wait for capacity held by the parent
        // that is waiting for the children. Release the read permit first;
        // expanded payloads remain bounded by ResourceBudget and nested
        // document parts by Package/entry limits.
        if kind.retains_input_while_expanding() {
            drop(permit);
        }

        let mut node = self.compare_bytes_internal(
            label,
            &hint,
            &left_bytes,
            &right_bytes,
            left_kind,
            right_kind,
            depth,
            ctx,
        )?;
        if kind == FileKind::Pdf {
            self.attach_pdf_visual(&mut node, left, right, label, ctx);
        }
        if node.status == DiffStatus::Equal {
            ctx.summary.files_equal += 1;
        } else {
            ctx.summary.files_changed += 1;
        }
        Ok(node)
    }

    fn attach_pdf_visual(
        &self,
        node: &mut DiffNode,
        left: &Path,
        right: &Path,
        label: &str,
        ctx: &mut Ctx,
    ) {
        let (Some(options), Some(base_output)) = (
            self.options.pdf_visual.as_ref(),
            self.options.pdf_visual_output.as_ref(),
        ) else {
            return;
        };
        let suffix = label
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
            .collect::<String>();
        let output_dir = if suffix.is_empty() {
            base_output.clone()
        } else {
            base_output.join(suffix)
        };
        match crate::pdf_visual::visual_pdf_diff(left, right, &output_dir, options) {
            Ok(report) => {
                let DiffBody::Document(document) = &mut node.body else {
                    ctx.warn(format!(
                        "{}: PDF visual comparison produced a non-document result",
                        display_label(label)
                    ));
                    return;
                };
                document.visual = Some(DocumentVisual {
                    kind: "pdf".to_string(),
                    output_dir: report.output_dir.display().to_string(),
                    report_html: report.report_html.display().to_string(),
                    report_pdf: report.report_pdf.display().to_string(),
                    report_json: report.report_json.display().to_string(),
                    pages_compared: report.pages_compared,
                    pages_added: report.pages_added,
                    pages_removed: report.pages_removed,
                    changed_pages: report.changed_pages,
                    pages: report
                        .pages
                        .iter()
                        .map(|page| VisualPage {
                            page: page.page,
                            differing_pixels: page.differing_pixels,
                            total_pixels: page.total_pixels,
                            diff_ratio: page.diff_ratio,
                        })
                        .collect(),
                });
            }
            Err(error) => ctx.warn(format!(
                "{}: PDF visual comparison failed ({error}); text diff is still available",
                display_label(label)
            )),
        }
    }

    /// Streaming byte comparison for files above the detailed-diff limit: a
    /// definitive same/different answer without loading either side.
    #[allow(clippy::too_many_arguments)]
    fn stream_summary_node(
        &self,
        label: &str,
        left: &Path,
        right: &Path,
        left_len: u64,
        right_len: u64,
        kind: FileKind,
        ctx: &mut Ctx,
    ) -> Result<DiffNode> {
        let stream = compare_streams(left, right)?;
        ctx.summary.files_compared += 1;
        if stream.equal {
            ctx.summary.files_equal += 1;
            return Ok(equal_node(
                label,
                kind,
                "files are byte-identical (large-file streaming check)",
            ));
        }
        ctx.summary.files_changed += 1;
        ctx.summary.bytes_differing += stream.differing_bytes;
        ctx.warn(format!(
            "{}: file exceeds the {} byte detailed-diff limit; only a summary is shown",
            display_label(label),
            self.options.max_file_bytes
        ));
        Ok(DiffNode::new(
            label,
            DiffStatus::Different,
            DiffBody::Binary(BinaryDiff {
                left_len,
                right_len,
                differing_bytes: stream.differing_bytes,
                first_difference: stream.first_difference,
                rows: Vec::new(),
                truncated: true,
            }),
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn compare_bytes_internal(
        &self,
        label: &str,
        hint: &str,
        left: &[u8],
        right: &[u8],
        left_kind: FileKind,
        right_kind: FileKind,
        depth: usize,
        ctx: &mut Ctx,
    ) -> Result<DiffNode> {
        let kind = effective_kind(left_kind, right_kind);
        if left == right {
            return Ok(equal_node(label, kind, "contents are byte-identical"));
        }

        // Parser registry: dispatch by the effective kind. Kinds without a
        // registered parser fall back to a binary comparison.
        let args = CompareArgs {
            label,
            hint,
            left,
            right,
            left_kind,
            right_kind,
            kind,
            depth,
        };
        match parser_for(kind) {
            Some(parser) => parser.compare_bytes(self, args, ctx),
            None => Ok(self.compare_binary_bytes(label, left, right, ctx)),
        }
    }

    fn compare_token_bytes(
        &self,
        label: &str,
        hint: &str,
        left: &[u8],
        right: &[u8],
        ctx: &mut Ctx,
    ) -> Option<DiffNode> {
        let (language_name, language) = tokens::language_for_name(hint)?;
        let left_decoded = text::decode_text(left);
        let right_decoded = text::decode_text(right);
        let tokenize_options = tokens::TokenizeOptions {
            ignore_comments: self.options.ignore_comments,
            max_tokens: 200_000,
        };
        let left_tokens = match tokens::tokenize(&left_decoded.text, &language, tokenize_options) {
            Ok(tokens) => tokens,
            Err(error) => {
                ctx.warn(format!(
                    "{}: tokenization failed ({error}); falling back to text diff",
                    display_label(label)
                ));
                return None;
            }
        };
        let right_tokens = match tokens::tokenize(&right_decoded.text, &language, tokenize_options)
        {
            Ok(tokens) => tokens,
            Err(error) => {
                ctx.warn(format!(
                    "{}: tokenization failed ({error}); falling back to text diff",
                    display_label(label)
                ));
                return None;
            }
        };

        let result = tokens::compare_token_streams(
            language_name,
            &left_tokens,
            &right_tokens,
            self.options.max_hunks,
        );
        ctx.summary.tokens_added += result.added_tokens;
        ctx.summary.tokens_removed += result.removed_tokens;
        let status = if result.added_tokens == 0 && result.removed_tokens == 0 {
            DiffStatus::Equal
        } else {
            DiffStatus::Different
        };
        Some(DiffNode::new(
            label,
            status,
            DiffBody::Tokens(TokenDiff {
                language: result.language,
                added_tokens: result.added_tokens,
                removed_tokens: result.removed_tokens,
                chunks: result
                    .chunks
                    .into_iter()
                    .map(|chunk| TokenChunk {
                        tag: chunk.tag,
                        text: chunk.text,
                    })
                    .collect(),
                truncated: result.truncated,
            }),
        ))
    }

    fn compare_audio_files(
        &self,
        left: &Path,
        right: &Path,
        label: &str,
        ctx: &mut Ctx,
    ) -> Option<DiffNode> {
        use lofty::file::{AudioFile, TaggedFileExt};

        let left_file = match lofty::read_from_path(left) {
            Ok(file) => file,
            Err(error) => {
                ctx.warn(format!(
                    "{}: could not read audio metadata ({error}); falling back to binary",
                    display_label(label)
                ));
                return None;
            }
        };
        let right_file = match lofty::read_from_path(right) {
            Ok(file) => file,
            Err(error) => {
                ctx.warn(format!(
                    "{}: could not read audio metadata ({error}); falling back to binary",
                    display_label(label)
                ));
                return None;
            }
        };

        let left_properties = left_file.properties();
        let right_properties = right_file.properties();
        let mut property_changes = 0;
        if left_properties.duration() != right_properties.duration() {
            property_changes += 1;
        }
        if left_properties.audio_bitrate() != right_properties.audio_bitrate() {
            property_changes += 1;
        }
        if left_properties.sample_rate() != right_properties.sample_rate() {
            property_changes += 1;
        }
        if left_properties.channels() != right_properties.channels() {
            property_changes += 1;
        }

        let left_tags = audio_tags(&left_file);
        let right_tags = audio_tags(&right_file);
        let keys: BTreeSet<String> = left_tags.keys().chain(right_tags.keys()).cloned().collect();
        let mut tag_changes = Vec::new();
        for key in keys {
            let left_value = left_tags.get(&key).cloned();
            let right_value = right_tags.get(&key).cloned();
            if left_value != right_value {
                tag_changes.push(AudioTagChange {
                    key,
                    left: left_value,
                    right: right_value,
                });
            }
        }
        ctx.summary.audio_tag_changes += tag_changes.len();
        ctx.summary.audio_property_changes += property_changes;

        let content_differs = !compare_streams(left, right)
            .map(|result| result.equal)
            .unwrap_or(false);
        let waveform = match (
            crate::diff::media::audio_waveform(left),
            crate::diff::media::audio_waveform(right),
        ) {
            (Ok(left), Ok(right)) => Some(crate::diff::media::waveform_diff(left, right)),
            _ => None,
        };
        let status = if tag_changes.is_empty() && property_changes == 0 && !content_differs {
            DiffStatus::Equal
        } else {
            DiffStatus::Different
        };
        Some(DiffNode::new(
            label,
            status,
            DiffBody::Audio(AudioDiff {
                left_format: format!("{:?}", left_file.file_type()),
                right_format: format!("{:?}", right_file.file_type()),
                left_duration_ms: left_properties.duration().as_millis(),
                right_duration_ms: right_properties.duration().as_millis(),
                left_bitrate: left_properties.audio_bitrate(),
                right_bitrate: right_properties.audio_bitrate(),
                left_sample_rate: left_properties.sample_rate(),
                right_sample_rate: right_properties.sample_rate(),
                left_channels: left_properties.channels(),
                right_channels: right_properties.channels(),
                tags: tag_changes,
                content_differs,
                waveform,
            }),
        ))
    }

    fn compare_video_files(
        &self,
        left: &Path,
        right: &Path,
        label: &str,
        ctx: &mut Ctx,
    ) -> Option<DiffNode> {
        let left_value = match crate::diff::media::video_value(left) {
            Ok(value) => value,
            Err(error) => {
                ctx.warn(format!(
                    "{}: could not read video metadata ({error}); falling back to binary",
                    display_label(label)
                ));
                return None;
            }
        };
        let right_value = match crate::diff::media::video_value(right) {
            Ok(value) => value,
            Err(error) => {
                ctx.warn(format!(
                    "{}: could not read video metadata ({error}); falling back to binary",
                    display_label(label)
                ));
                return None;
            }
        };
        let diff = structured::compare_values(
            "video",
            &left_value,
            &right_value,
            self.options.max_json_changes,
        );
        ctx.summary.structured_changes += diff.changes.len();
        let status = if diff.changes.is_empty() {
            DiffStatus::Equal
        } else {
            DiffStatus::Different
        };
        Some(DiffNode::new(label, status, DiffBody::Structured(diff)))
    }

    fn compare_pdf_ocr(
        &self,
        left: &Path,
        right: &Path,
        label: &str,
        ctx: &mut Ctx,
    ) -> Option<DiffNode> {
        let ocr_options = self.options.ocr.as_ref()?;
        if ocr_options.pdftoppm.is_none() {
            ctx.warn(format!(
                "{}: PDF OCR requires pdftoppm; using PDF text layer",
                display_label(label)
            ));
            return None;
        }
        match (
            crate::ocr::ocr_pdf_text(left, ocr_options),
            crate::ocr::ocr_pdf_text(right, ocr_options),
        ) {
            (Ok(left_text), Ok(right_text))
                if !left_text.trim().is_empty() || !right_text.trim().is_empty() =>
            {
                ctx.warn(format!(
                    "{}: compared OCR text from rendered PDF pages ({})",
                    display_label(label),
                    ocr_options.language
                ));
                Some(self.compare_text_strings(
                    label,
                    &left_text,
                    &right_text,
                    "poppler+tesseract",
                    "poppler+tesseract",
                    "pdf-ocr",
                    FileKind::Pdf,
                    ctx,
                ))
            }
            (Ok(_), Ok(_)) => {
                ctx.warn(format!(
                    "{}: PDF OCR produced no text; using PDF text layer",
                    display_label(label)
                ));
                None
            }
            (left_result, right_result) => {
                let error = left_result
                    .err()
                    .or_else(|| right_result.err())
                    .map(|error| error.to_string())
                    .unwrap_or_default();
                ctx.warn(format!(
                    "{}: PDF OCR failed ({error}); using PDF text layer",
                    display_label(label)
                ));
                None
            }
        }
    }

    fn compare_tree_bytes(
        &self,
        label: &str,
        hint: &str,
        left: &[u8],
        right: &[u8],
        ctx: &mut Ctx,
    ) -> Option<DiffNode> {
        let (language_name, language) = tokens::language_for_name(hint)?;
        let left_decoded = text::decode_text(left);
        let right_decoded = text::decode_text(right);
        let options = tree_diff::TreeOptions {
            ignore_comments: self.options.ignore_comments,
            ..tree_diff::TreeOptions::default()
        };
        let left_tree = match tree_diff::build_tree(&left_decoded.text, &language, options) {
            Ok(tree) => tree,
            Err(error) => {
                ctx.warn(format!(
                    "{}: tree diff failed ({error}); falling back to text diff",
                    display_label(label)
                ));
                return None;
            }
        };
        let right_tree = match tree_diff::build_tree(&right_decoded.text, &language, options) {
            Ok(tree) => tree,
            Err(error) => {
                ctx.warn(format!(
                    "{}: tree diff failed ({error}); falling back to text diff",
                    display_label(label)
                ));
                return None;
            }
        };
        let result = if self.options.mode == DiffMode::Graph {
            tree_diff::compare_trees_graph(
                language_name,
                &left_tree,
                &right_tree,
                self.options.max_hunks,
            )
        } else {
            tree_diff::compare_trees(
                language_name,
                &left_tree,
                &right_tree,
                self.options.max_hunks,
            )
        };
        ctx.summary.tree_added += result.added_nodes;
        ctx.summary.tree_removed += result.removed_nodes;
        ctx.summary.tree_modified += result.modified_nodes;
        ctx.summary.tree_moved += result.moved_nodes;
        let status = if result.added_nodes == 0
            && result.removed_nodes == 0
            && result.modified_nodes == 0
            && result.moved_nodes == 0
        {
            DiffStatus::Equal
        } else {
            DiffStatus::Different
        };
        Some(DiffNode::new(
            label,
            status,
            DiffBody::Tree(TreeDiff {
                language: result.language,
                added_nodes: result.added_nodes,
                removed_nodes: result.removed_nodes,
                modified_nodes: result.modified_nodes,
                moved_nodes: result.moved_nodes,
                chunks: result
                    .chunks
                    .into_iter()
                    .map(|chunk| TreeChunk {
                        tag: chunk.tag,
                        path: chunk.path,
                        text: chunk.text,
                        depth: chunk.depth,
                    })
                    .collect(),
                truncated: result.truncated,
            }),
        ))
    }

    fn compare_text_bytes(
        &self,
        label: &str,
        left: &[u8],
        right: &[u8],
        format: &str,
        kind: FileKind,
        ctx: &mut Ctx,
    ) -> DiffNode {
        let left_decoded = text::decode_text(left);
        let right_decoded = text::decode_text(right);
        self.compare_text_strings(
            label,
            &left_decoded.text,
            &right_decoded.text,
            &left_decoded.encoding,
            &right_decoded.encoding,
            format,
            kind,
            ctx,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn compare_text_strings(
        &self,
        label: &str,
        left_text: &str,
        right_text: &str,
        encoding_left: &str,
        encoding_right: &str,
        format: &str,
        kind: FileKind,
        ctx: &mut Ctx,
    ) -> DiffNode {
        if left_text == right_text {
            return equal_node(
                label,
                kind,
                &format!(
                    "{format} content is equal (encodings: {encoding_left} / {encoding_right})"
                ),
            );
        }

        let mut diff = text::compare_text_options(
            left_text,
            right_text,
            self.options.context,
            self.options.max_hunks,
            self.options.ignore_whitespace,
            self.options.ignore_case,
        );
        diff.format = format.to_string();
        diff.encoding_left = encoding_left.to_string();
        diff.encoding_right = encoding_right.to_string();

        ctx.summary.lines_added += diff.added_lines;
        ctx.summary.lines_removed += diff.removed_lines;

        let status = if diff.added_lines == 0 && diff.removed_lines == 0 {
            DiffStatus::Equal
        } else {
            DiffStatus::Different
        };
        DiffNode::new(label, status, DiffBody::Text(diff))
    }

    #[allow(clippy::too_many_arguments)]
    fn compare_structured_bytes(
        &self,
        label: &str,
        hint: &str,
        left: &[u8],
        right: &[u8],
        left_kind: FileKind,
        right_kind: FileKind,
        ctx: &mut Ctx,
    ) -> DiffNode {
        let format = if left_kind == right_kind {
            structured_format(left_kind, hint)
        } else {
            format!(
                "{}+{}",
                structured::format_label(left_kind),
                structured::format_label(right_kind)
            )
        };
        match (
            structured::parse_structured(left_kind, left),
            structured::parse_structured(right_kind, right),
        ) {
            (Ok(left_value), Ok(right_value)) => {
                let openapi = structured::looks_like_openapi(&left_value)
                    && structured::looks_like_openapi(&right_value);
                let diff = if openapi {
                    structured::compare_openapi(
                        &left_value,
                        &right_value,
                        self.options.max_json_changes,
                    )
                } else {
                    structured::compare_values(
                        &format,
                        &left_value,
                        &right_value,
                        self.options.max_json_changes,
                    )
                };
                ctx.summary.structured_changes += diff.changes.len();
                let status = if diff.changes.is_empty() {
                    DiffStatus::Equal
                } else {
                    DiffStatus::Different
                };
                DiffNode::new(label, status, DiffBody::Structured(diff))
            }
            (left_result, right_result) => {
                let reason = left_result
                    .err()
                    .or_else(|| right_result.err())
                    .unwrap_or_default();
                ctx.warn(format!(
                    "{}: invalid {format} ({reason}); falling back to text diff",
                    display_label(label)
                ));
                self.compare_text_bytes(label, left, right, &format, left_kind, ctx)
            }
        }
    }

    fn compare_sqlite_bytes(
        &self,
        label: &str,
        left: &[u8],
        right: &[u8],
        ctx: &mut Ctx,
    ) -> DiffNode {
        match (sqlite::parse_sqlite(left), sqlite::parse_sqlite(right)) {
            (Ok(left_value), Ok(right_value)) => {
                let diff = structured::compare_values(
                    "sqlite",
                    &left_value,
                    &right_value,
                    self.options.max_json_changes,
                );
                ctx.summary.structured_changes += diff.changes.len();
                let status = if diff.changes.is_empty() {
                    DiffStatus::Equal
                } else {
                    DiffStatus::Different
                };
                DiffNode::new(label, status, DiffBody::Structured(diff))
            }
            (left_result, right_result) => {
                let reason = left_result
                    .err()
                    .or_else(|| right_result.err())
                    .unwrap_or_default();
                ctx.warn(format!(
                    "{}: invalid sqlite database ({reason}); falling back to binary diff",
                    display_label(label)
                ));
                self.compare_binary_bytes(label, left, right, ctx)
            }
        }
    }

    fn compare_executable_bytes(
        &self,
        label: &str,
        left: &[u8],
        right: &[u8],
        ctx: &mut Ctx,
    ) -> DiffNode {
        match (
            executable::parse_executable(left),
            executable::parse_executable(right),
        ) {
            (Ok(left_value), Ok(right_value)) => {
                let format = left_value
                    .get("format")
                    .and_then(|value| value.as_str())
                    .unwrap_or("executable")
                    .to_string();
                let mut diff = structured::compare_values(
                    &format,
                    &left_value,
                    &right_value,
                    self.options.max_json_changes,
                );
                if format != "mach-o-fat" {
                    add_executable_byte_changes(
                        &mut diff,
                        &left_value,
                        &right_value,
                        left,
                        right,
                        self.options.max_json_changes,
                    );
                }
                ctx.summary.structured_changes += diff.changes.len();
                let status = if diff.changes.is_empty() {
                    DiffStatus::Equal
                } else {
                    DiffStatus::Different
                };
                DiffNode::new(label, status, DiffBody::Structured(diff))
            }
            (left_result, right_result) => {
                let reason = left_result
                    .err()
                    .or_else(|| right_result.err())
                    .unwrap_or_default();
                ctx.warn(format!(
                    "{}: invalid executable ({reason}); falling back to binary diff",
                    display_label(label)
                ));
                self.compare_binary_bytes(label, left, right, ctx)
            }
        }
    }

    fn compare_dex_bytes(&self, label: &str, left: &[u8], right: &[u8], ctx: &mut Ctx) -> DiffNode {
        if self.options.decompile.is_none() {
            ctx.warn(format!(
                "{}: DEX decompilation requires --detail full or explicit decompile options; falling back to binary",
                display_label(label)
            ));
            return self.compare_binary_bytes(label, left, right, ctx);
        }
        match crate::diff::dex::compare_dex(
            left,
            right,
            self.options.context,
            self.options.max_hunks,
        ) {
            Ok(diff) => {
                ctx.summary.lines_added += diff.added_lines;
                ctx.summary.lines_removed += diff.removed_lines;
                let status = if diff.added_lines == 0 && diff.removed_lines == 0 {
                    DiffStatus::Equal
                } else {
                    DiffStatus::Different
                };
                DiffNode::new(label, status, DiffBody::Text(diff))
            }
            Err(reason) => {
                ctx.warn(format!(
                    "{}: DEX decompilation failed ({reason}); falling back to binary",
                    display_label(label)
                ));
                self.compare_binary_bytes(label, left, right, ctx)
            }
        }
    }

    fn compare_font_bytes(
        &self,
        label: &str,
        left: &[u8],
        right: &[u8],
        ctx: &mut Ctx,
    ) -> DiffNode {
        match (font::parse_font(left), font::parse_font(right)) {
            (Ok(left_value), Ok(right_value)) => {
                let diff = structured::compare_values(
                    "font",
                    &left_value,
                    &right_value,
                    self.options.max_json_changes,
                );
                ctx.summary.structured_changes += diff.changes.len();
                let status = if diff.changes.is_empty() {
                    DiffStatus::Equal
                } else {
                    DiffStatus::Different
                };
                DiffNode::new(label, status, DiffBody::Structured(diff))
            }
            (left_result, right_result) => {
                let reason = left_result
                    .err()
                    .or_else(|| right_result.err())
                    .unwrap_or_default();
                ctx.warn(format!(
                    "{}: invalid font ({reason}); falling back to binary diff",
                    display_label(label)
                ));
                self.compare_binary_bytes(label, left, right, ctx)
            }
        }
    }

    fn compare_notebook_bytes(
        &self,
        label: &str,
        left: &[u8],
        right: &[u8],
        ctx: &mut Ctx,
    ) -> DiffNode {
        match crate::diff::notebook::compare_notebooks(left, right, self.options.max_json_changes) {
            Ok(diff) => {
                ctx.summary.notebook_cells_added += diff.cells_added;
                ctx.summary.notebook_cells_removed += diff.cells_removed;
                ctx.summary.notebook_cells_changed += diff.changed_cells.len();
                ctx.summary.notebook_cells_moved += diff.cells_moved;
                let status = if diff.cells_added == 0
                    && diff.cells_removed == 0
                    && diff.cells_moved == 0
                    && diff.changed_cells.is_empty()
                {
                    DiffStatus::Equal
                } else {
                    DiffStatus::Different
                };
                DiffNode::new(label, status, DiffBody::Notebook(diff))
            }
            Err(error) => {
                ctx.warn(format!(
                    "{}: invalid notebook ({error}); falling back to text diff",
                    display_label(label)
                ));
                self.compare_text_bytes(label, left, right, "notebook", FileKind::Notebook, ctx)
            }
        }
    }

    fn compare_table_bytes(
        &self,
        label: &str,
        left: &[u8],
        right: &[u8],
        hint: &str,
        ctx: &mut Ctx,
    ) -> DiffNode {
        let format = if hint.to_ascii_lowercase().ends_with(".tsv") {
            "tsv"
        } else {
            "csv"
        };
        let analysis_limit = self.options.max_table_rows;
        let table_limit = self
            .options
            .concise_limit
            .map(|limit| limit.min(analysis_limit))
            .unwrap_or(analysis_limit);
        let display_only = self
            .options
            .concise_limit
            .is_some_and(|limit| limit < analysis_limit);
        match table::compare_tables_with_options(
            left,
            right,
            format,
            &table::TableCompareOptions {
                key_column: self.options.key_column.as_deref(),
                ignore_columns: &self.options.ignore_columns,
                numeric_tolerance: self.options.numeric_tolerance,
            },
            table_limit,
        ) {
            Ok(diff) => {
                let rows_added = diff
                    .rows
                    .iter()
                    .filter(|row| row.kind == RowChangeKind::Added)
                    .count();
                let rows_removed = diff
                    .rows
                    .iter()
                    .filter(|row| row.kind == RowChangeKind::Removed)
                    .count();
                let rows_changed = diff
                    .rows
                    .iter()
                    .filter(|row| matches!(row.kind, RowChangeKind::Changed | RowChangeKind::Moved))
                    .count();
                let cells_changed: usize = diff.rows.iter().map(|row| row.cells.len()).sum();
                ctx.summary.table_rows_added += rows_added;
                ctx.summary.table_rows_removed += rows_removed;
                ctx.summary.table_rows_changed += rows_changed;
                ctx.summary.table_cells_changed += cells_changed;
                if let Some(key) = &diff.implicit_key {
                    ctx.warn(format!(
                        "{}: inferred unique column {key:?} for row alignment; pass --key to override",
                        display_label(label)
                    ));
                }
                if !diff.duplicate_keys.is_empty() {
                    let shown = diff
                        .duplicate_keys
                        .iter()
                        .take(10)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ");
                    ctx.warn(format!(
                        "{}: duplicate key value(s) in {}: {shown}; occurrences are compared separately",
                        display_label(label),
                        self.options
                            .key_column
                            .as_deref()
                            .unwrap_or("the key column"),
                    ));
                }
                if diff.input_truncated {
                    ctx.warn(format!(
                        "{}: input row cap reached; only the first {} rows were parsed",
                        display_label(label),
                        self.options.max_table_rows.saturating_mul(64).max(10_000)
                    ));
                }
                if diff.truncated && !display_only {
                    ctx.warn(format!(
                        "{}: table change limit ({}) reached; some rows were not reported",
                        display_label(label),
                        self.options.max_table_rows
                    ));
                }

                let truncated = diff.input_truncated || (diff.truncated && !display_only);
                let status = if diff.rows.is_empty()
                    && diff.columns_added.is_empty()
                    && diff.columns_removed.is_empty()
                {
                    DiffStatus::Equal
                } else {
                    DiffStatus::Different
                };
                complete_unless(
                    DiffNode::new(label, status, DiffBody::Table(diff)),
                    truncated,
                )
            }
            Err(error) => {
                ctx.warn(format!(
                    "{}: could not parse table ({error}); falling back to text diff",
                    display_label(label)
                ));
                self.compare_text_bytes(label, left, right, format, FileKind::Csv, ctx)
            }
        }
    }

    fn compare_workbook_bytes(
        &self,
        label: &str,
        left: &[u8],
        right: &[u8],
        hint: &str,
        ctx: &mut Ctx,
    ) -> DiffNode {
        let format = spreadsheet_format(hint);
        let analysis_limit = self.options.max_workbook_cells;
        let workbook_limit = self
            .options
            .concise_limit
            .map(|limit| limit.min(analysis_limit))
            .unwrap_or(analysis_limit);
        let display_only = self
            .options
            .concise_limit
            .is_some_and(|limit| limit < analysis_limit);
        match workbook::compare_workbooks_with_budget(
            left,
            right,
            format,
            workbook_limit,
            self.options.compare_formulas,
            &self.options.ignore_columns,
            ctx.budget.clone(),
            self.options.max_file_bytes,
        ) {
            Ok(diff) => {
                let cells_changed: usize = diff.sheets.iter().map(|sheet| sheet.cells.len()).sum();
                ctx.summary.workbook_cells_changed += cells_changed;
                if !diff.sheets_moved.is_empty() {
                    ctx.warn(format!(
                        "{}: worksheet order changed for {} sheet(s)",
                        display_label(label),
                        diff.sheets_moved.len()
                    ));
                }
                if diff.input_truncated {
                    ctx.warn(format!(
                        "{}: worksheet range exceeds the safe input limit; large sparse sheets were skipped",
                        display_label(label)
                    ));
                }
                if diff.truncated && !display_only {
                    ctx.warn(format!(
                        "{}: workbook change limit ({}) reached; some cells were not reported",
                        display_label(label),
                        self.options.max_workbook_cells
                    ));
                }
                let truncated = diff.input_truncated || (diff.truncated && !display_only);
                let hidden_changed = diff.sheets.iter().any(|sheet| {
                    sheet.left_hidden != sheet.right_hidden
                        || !sheet.merges_added.is_empty()
                        || !sheet.merges_removed.is_empty()
                        || !sheet.hidden_dimensions.is_empty()
                        || !sheet.drawings_added.is_empty()
                        || !sheet.drawings_removed.is_empty()
                        || !sheet.chart_changes.is_empty()
                        || !sheet.conditional_formats_added.is_empty()
                        || !sheet.conditional_formats_removed.is_empty()
                });
                let status = if diff.sheets_added.is_empty()
                    && diff.sheets_removed.is_empty()
                    && diff.sheets_moved.is_empty()
                    && diff.sheets_renamed.is_empty()
                    && diff.defined_names.is_empty()
                    && !hidden_changed
                    && cells_changed == 0
                {
                    DiffStatus::Equal
                } else {
                    DiffStatus::Different
                };
                complete_unless(
                    DiffNode::new(label, status, DiffBody::Workbook(diff)),
                    truncated,
                )
            }
            Err(error) => {
                if let Some(node) = budget_error_node(label, &error, ctx) {
                    return node;
                }
                ctx.warn(format!(
                    "{}: could not read workbook ({error}); falling back to binary",
                    display_label(label)
                ));
                self.compare_binary_bytes(label, left, right, ctx)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn compare_document_bytes(
        &self,
        label: &str,
        left: &[u8],
        right: &[u8],
        kind: FileKind,
        left_kind: FileKind,
        right_kind: FileKind,
        ctx: &mut Ctx,
    ) -> DiffNode {
        if kind == FileKind::DocumentCross {
            match documents::compare_mixed_documents_with_budget(
                left_kind,
                left,
                right_kind,
                right,
                self.options.max_doc_blocks,
                self.options.max_table_rows,
                ctx.budget.clone(),
                self.options.max_file_bytes,
            ) {
                Ok(diff) => {
                    let status = document_status(&diff);
                    let truncated = diff.truncated;
                    return complete_unless(
                        DiffNode::new(label, status, DiffBody::Document(diff)),
                        truncated,
                    );
                }
                Err(error) => {
                    if let Some(node) = budget_error_node(label, &error, ctx) {
                        return node;
                    }
                }
            }
            let left_text = extract_document_text(left_kind, left);
            let right_text = extract_document_text(right_kind, right);
            match (left_text, right_text) {
                (Ok(left_text), Ok(right_text))
                    if !left_text.trim().is_empty() || !right_text.trim().is_empty() =>
                {
                    let format = format!("{}-to-{}", left_kind.label(), right_kind.label());
                    return self.compare_text_strings(
                        label,
                        &left_text,
                        &right_text,
                        "extracted text",
                        "extracted text",
                        &format,
                        FileKind::Text,
                        ctx,
                    );
                }
                _ => {
                    ctx.warn(format!(
                        "{}: could not extract cross-format document text; falling back to binary",
                        display_label(label)
                    ));
                    return self.compare_binary_bytes(label, left, right, ctx);
                }
            }
        }
        if kind == FileKind::Docx {
            let block_limit = self
                .options
                .concise_limit
                .map(|limit| limit.min(self.options.max_doc_blocks))
                .unwrap_or(self.options.max_doc_blocks);
            let table_limit = self
                .options
                .concise_limit
                .map(|limit| limit.min(self.options.max_table_rows))
                .unwrap_or(self.options.max_table_rows);
            let display_only = self.options.concise_limit.is_some_and(|limit| {
                limit < self.options.max_doc_blocks || limit < self.options.max_table_rows
            });
            match documents::compare_docx_documents_with_budget(
                left,
                right,
                block_limit,
                table_limit,
                ctx.budget.clone(),
                self.options.max_file_bytes,
            ) {
                Ok(diff)
                    if !diff.blocks.is_empty()
                        || !diff.tables.is_empty()
                        || !diff.media.is_empty()
                        || diff.truncated =>
                {
                    if diff.truncated && !display_only {
                        ctx.warn(format!(
                            "{}: document block limit ({}) reached; some blocks were not compared",
                            display_label(label),
                            self.options.max_doc_blocks
                        ));
                    }
                    let status = document_status(&diff);
                    let truncated = diff.truncated && !display_only;
                    return complete_unless(
                        DiffNode::new(label, status, DiffBody::Document(diff)),
                        truncated,
                    );
                }
                Ok(_) => ctx.warn(format!(
                    "{}: DOCX has no comparable blocks; falling back to extracted text",
                    display_label(label)
                )),
                Err(error) => {
                    if let Some(node) = budget_error_node(label, &error, ctx) {
                        return node;
                    }
                    ctx.warn(format!(
                        "{}: could not parse DOCX structure ({error}); falling back to extracted text",
                        display_label(label)
                    ));
                }
            }
        }
        if kind == FileKind::Pptx {
            let slide_limit = self
                .options
                .concise_limit
                .map(|limit| limit.min(self.options.max_slides))
                .unwrap_or(self.options.max_slides);
            let table_limit = self
                .options
                .concise_limit
                .map(|limit| limit.min(self.options.max_table_rows))
                .unwrap_or(self.options.max_table_rows);
            let display_only = self.options.concise_limit.is_some_and(|limit| {
                limit < self.options.max_slides || limit < self.options.max_table_rows
            });
            match documents::compare_pptx_documents_with_budget(
                left,
                right,
                slide_limit,
                table_limit,
                ctx.budget.clone(),
                self.options.max_file_bytes,
            ) {
                Ok(diff) => {
                    if diff.truncated && !display_only {
                        ctx.warn(format!(
                            "{}: slide limit ({}) reached; some slides were not compared",
                            display_label(label),
                            self.options.max_slides
                        ));
                    }
                    let status = if diff.slides.is_empty() {
                        DiffStatus::Equal
                    } else {
                        DiffStatus::Different
                    };
                    let truncated = diff.truncated && !display_only;
                    return complete_unless(
                        DiffNode::new(label, status, DiffBody::Presentation(diff)),
                        truncated,
                    );
                }
                Err(error) => {
                    if let Some(node) = budget_error_node(label, &error, ctx) {
                        return node;
                    }
                    ctx.warn(format!(
                        "{}: could not parse PPTX structure ({error}); falling back to extracted text",
                        display_label(label)
                    ));
                }
            }
        }

        if matches!(kind, FileKind::Odt | FileKind::Odp) {
            let block_limit = self
                .options
                .concise_limit
                .map(|limit| limit.min(self.options.max_doc_blocks))
                .unwrap_or(self.options.max_doc_blocks);
            let table_limit = self
                .options
                .concise_limit
                .map(|limit| limit.min(self.options.max_table_rows))
                .unwrap_or(self.options.max_table_rows);
            let display_only = self.options.concise_limit.is_some_and(|limit| {
                limit < self.options.max_doc_blocks || limit < self.options.max_table_rows
            });
            match documents::compare_odf_documents_with_budget(
                left,
                right,
                kind.label(),
                block_limit,
                table_limit,
                ctx.budget.clone(),
                self.options.max_file_bytes,
            ) {
                Ok(diff) => {
                    if diff.truncated && !display_only {
                        ctx.warn(format!(
                            "{}: document block limit ({}) reached; some blocks were not compared",
                            display_label(label),
                            self.options.max_doc_blocks
                        ));
                    }
                    let status = document_status(&diff);
                    let truncated = diff.truncated && !display_only;
                    return complete_unless(
                        DiffNode::new(label, status, DiffBody::Document(diff)),
                        truncated,
                    );
                }
                Err(error) => {
                    if let Some(node) = budget_error_node(label, &error, ctx) {
                        return node;
                    }
                    ctx.warn(format!(
                        "{}: could not parse {} structure ({error}); falling back to extracted text",
                        display_label(label),
                        kind.label()
                    ));
                }
            }
        }

        if kind == FileKind::Pdf {
            match documents::compare_pdf_documents(left, right) {
                Ok(diff) => {
                    let status = document_status(&diff);
                    return DiffNode::new(label, status, DiffBody::Document(diff));
                }
                Err(error) => ctx.warn(format!(
                    "{}: could not parse PDF pages ({error}); falling back to extracted text",
                    display_label(label)
                )),
            }
        }
        if kind == FileKind::Ofd {
            match documents::compare_ofd_documents(left, right) {
                Ok(diff) => {
                    let status = document_status(&diff);
                    return DiffNode::new(label, status, DiffBody::Document(diff));
                }
                Err(error) => ctx.warn(format!(
                    "{}: could not parse OFD pages ({error}); falling back to extracted text",
                    display_label(label)
                )),
            }
        }

        let (format, left_text, right_text) = match kind {
            FileKind::Pdf => (
                "pdf",
                documents::extract_pdf_text(left),
                documents::extract_pdf_text(right),
            ),
            FileKind::Ofd => (
                "ofd",
                documents::extract_ofd_text(left),
                documents::extract_ofd_text(right),
            ),
            FileKind::Docx => (
                "docx",
                documents::extract_docx_text(left),
                documents::extract_docx_text(right),
            ),
            FileKind::Pptx => (
                "pptx",
                documents::extract_pptx_text(left),
                documents::extract_pptx_text(right),
            ),
            _ => unreachable!(),
        };

        match (left_text, right_text) {
            (Ok(left_text), Ok(right_text))
                if !left_text.trim().is_empty() || !right_text.trim().is_empty() =>
            {
                self.compare_text_strings(
                    label,
                    &left_text,
                    &right_text,
                    "extracted text",
                    "extracted text",
                    format,
                    kind,
                    ctx,
                )
            }
            (Ok(_), Ok(_)) => {
                ctx.warn(format!(
                    "{}: no extractable text in {format}; falling back to binary",
                    display_label(label)
                ));
                self.compare_binary_bytes(label, left, right, ctx)
            }
            (left_result, right_result) => {
                let reason = left_result
                    .err()
                    .or_else(|| right_result.err())
                    .unwrap_or_default();
                ctx.warn(format!(
                    "{}: could not extract {format} text ({reason}); falling back to binary",
                    display_label(label)
                ));
                self.compare_binary_bytes(label, left, right, ctx)
            }
        }
    }

    fn compare_image_bytes(
        &self,
        label: &str,
        left: &[u8],
        right: &[u8],
        ctx: &mut Ctx,
    ) -> DiffNode {
        if let Some(ocr_options) = &self.options.ocr {
            match (
                crate::ocr::ocr_image_bytes(left, ocr_options),
                crate::ocr::ocr_image_bytes(right, ocr_options),
            ) {
                (Ok(left_text), Ok(right_text))
                    if !left_text.trim().is_empty() || !right_text.trim().is_empty() =>
                {
                    ctx.warn(format!(
                        "{}: compared OCR text from Tesseract ({})",
                        display_label(label),
                        ocr_options.language
                    ));
                    let node = self.compare_text_strings(
                        label,
                        &left_text,
                        &right_text,
                        "tesseract",
                        "tesseract",
                        "ocr",
                        FileKind::Image,
                        ctx,
                    );
                    if node.status == DiffStatus::Equal {
                        ctx.warn(format!(
                            "{}: OCR text is equal; visual differences may still exist",
                            display_label(label)
                        ));
                    }
                    return node;
                }
                (Ok(_), Ok(_)) => {
                    ctx.warn(format!(
                        "{}: OCR produced no text; falling back to pixel diff",
                        display_label(label)
                    ));
                }
                (left_result, right_result) => {
                    let error = left_result
                        .err()
                        .or_else(|| right_result.err())
                        .map(|error| error.to_string())
                        .unwrap_or_default();
                    ctx.warn(format!(
                        "{}: OCR failed ({error}); falling back to pixel diff",
                        display_label(label)
                    ));
                }
            }
        }
        match (
            image::load_from_memory(left),
            image::load_from_memory(right),
        ) {
            (Ok(left_image), Ok(right_image)) => {
                let diff = image_diff::compare_images(
                    &left_image,
                    &right_image,
                    self.options.image_threshold,
                );
                let status = if diff.differing_pixels == 0 {
                    DiffStatus::Equal
                } else {
                    DiffStatus::Different
                };
                DiffNode::new(label, status, DiffBody::Image(diff))
            }
            _ => {
                ctx.warn(format!(
                    "{}: could not decode image; falling back to binary",
                    display_label(label)
                ));
                self.compare_binary_bytes(label, left, right, ctx)
            }
        }
    }

    fn compare_binary_bytes(
        &self,
        label: &str,
        left: &[u8],
        right: &[u8],
        ctx: &mut Ctx,
    ) -> DiffNode {
        let diff = binary::compare_binary(left, right, self.options.max_rows);
        ctx.summary.bytes_differing += diff.differing_bytes;
        let status = if diff.differing_bytes == 0 {
            DiffStatus::Equal
        } else {
            DiffStatus::Different
        };
        DiffNode::new(label, status, DiffBody::Binary(diff))
    }

    fn compare_entry_sets<F>(
        &self,
        label: &str,
        keys: Vec<String>,
        limit: usize,
        kind: &str,
        ctx: &mut Ctx,
        compare_one: F,
    ) -> Result<EntryComparison>
    where
        F: Fn(&String, &mut Ctx) -> Result<DiffNode> + Sync + Send,
    {
        let total_entries = keys.len();
        let truncated = total_entries > limit;
        let mut keys = keys;
        keys.truncate(limit);
        if truncated {
            let skipped = if kind == "directory" {
                "files"
            } else {
                "entries"
            };
            ctx.warn(format!(
                "{}: {} entry limit ({}) reached; some {} were skipped",
                display_label(label),
                kind,
                limit,
                skipped
            ));
        }
        if let Some(progress) = &self.progress
            && progress.enabled()
        {
            progress.add_total(keys.len());
            progress.set_message(&format!("Comparing {}", display_label(label)));
        }
        let display = display_label(label).to_string();
        let excludes = ctx.excludes.clone();
        let budget = ctx.budget.clone();
        let process = |key: &String| {
            let mut local = Ctx::child(excludes.clone(), budget.clone());
            let node = compare_one(key, &mut local);
            if let Some(progress) = &self.progress
                && progress.enabled()
            {
                progress.inc(&format!("{display}: {key}"));
            }
            (node, local)
        };
        // Bounded scheduler: aggregate finished entries chunk by chunk instead
        // of collecting every result first, so only a bounded number of nodes
        // and worker contexts is resident at any time. Chunks are processed in
        // key order, keeping the report deterministic.
        let chunk_size = self.options.jobs.max(1).saturating_mul(8).max(1);
        let mut entries = Vec::with_capacity(keys.len());
        for chunk in keys.chunks(chunk_size) {
            let results: Vec<(Result<DiffNode>, Ctx)> = if self.options.jobs == 1 {
                chunk.iter().map(process).collect()
            } else {
                chunk.par_iter().map(process).collect()
            };
            for (result, local) in results {
                ctx.merge(local);
                entries.push(result?);
            }
        }
        Ok(EntryComparison {
            entries,
            total_entries,
            truncated,
        })
    }

    fn compare_directories(
        &self,
        left: &Path,
        right: &Path,
        label: &str,
        depth: usize,
        ctx: &mut Ctx,
    ) -> Result<DiffNode> {
        let left_walk = self.collect_files(left, ctx)?;
        let right_walk = self.collect_files(right, ctx)?;
        if left_walk.scan_truncated || right_walk.scan_truncated {
            ctx.warn(format!(
                "{}: directory entry limit ({}) reached; the scan stopped and the rest was not scanned",
                display_label(label),
                self.options.max_dir_entries
            ));
        }
        let left_files = &left_walk.files;
        let right_files = &right_walk.files;

        // Directories at the depth limit were not scanned. They are reported
        // as a single unsupported node each, like an archive at the limit,
        // instead of silently disappearing from the report.
        let unscanned: BTreeSet<String> = left_walk
            .unscanned
            .iter()
            .chain(right_walk.unscanned.iter())
            .cloned()
            .collect();

        let left_only: Vec<String> = left_files
            .keys()
            .filter(|key| !right_files.contains_key(*key))
            .cloned()
            .collect();
        let right_only: Vec<String> = right_files
            .keys()
            .filter(|key| !left_files.contains_key(*key))
            .cloned()
            .collect();
        let renames =
            detect_directory_renames(&self.gate, &left_only, &right_only, left_files, right_files);
        let renamed_right: BTreeSet<String> = renames.values().cloned().collect();
        let mut keys = merged_keys(left_files, right_files);
        keys.retain(|key| !renamed_right.contains(key));
        keys.extend(unscanned.iter().cloned());
        keys.sort();
        keys.dedup();
        let comparison = self.compare_entry_sets(
            label,
            keys,
            self.options.max_dir_entries,
            "directory",
            ctx,
            |key, local| {
                if unscanned.contains(key) {
                    local.summary.files_compared += 1;
                    return Ok(depth_limit_node(key, "directory depth limit reached"));
                }
                let right_path = right_files.get(key).or_else(|| {
                    renames
                        .get(key)
                        .and_then(|renamed| right_files.get(renamed))
                });
                match (left_files.get(key), right_path) {
                    (Some(left_path), Some(right_path)) => {
                        let mut node =
                            self.compare_paths(left_path, right_path, key, depth + 1, local)?;
                        let was_equal = node.status == DiffStatus::Equal;
                        if let Some(renamed) = renames.get(key) {
                            node.label = renamed.clone();
                            node.renamed_from = Some(key.clone());
                            node.status = DiffStatus::Different;
                        }
                        if was_equal && node.status != DiffStatus::Equal {
                            local.summary.files_equal = local.summary.files_equal.saturating_sub(1);
                            local.summary.files_changed += 1;
                        }
                        Ok(node)
                    }
                    (Some(left_path), None) => {
                        let kind = detect_file(left_path);
                        let size = fs::metadata(left_path).map(|m| m.len()).unwrap_or(0);
                        local.summary.files_removed += 1;
                        Ok(DiffNode::new(
                            key,
                            DiffStatus::Different,
                            DiffBody::OneSided(OneSidedInfo {
                                side: PresenceSide::LeftOnly,
                                kind,
                                size,
                            }),
                        ))
                    }
                    (None, Some(right_path)) => {
                        let kind = detect_file(right_path);
                        let size = fs::metadata(right_path).map(|m| m.len()).unwrap_or(0);
                        local.summary.files_added += 1;
                        Ok(DiffNode::new(
                            key,
                            DiffStatus::Different,
                            DiffBody::OneSided(OneSidedInfo {
                                side: PresenceSide::RightOnly,
                                kind,
                                size,
                            }),
                        ))
                    }
                    (None, None) => Ok(DiffNode::new(
                        key,
                        DiffStatus::Equal,
                        DiffBody::Equal(EqualInfo {
                            kind: FileKind::Unknown,
                            note: "missing on both sides".to_string(),
                        }),
                    )),
                }
            },
        )?;

        let truncated =
            comparison.truncated || left_walk.scan_truncated || right_walk.scan_truncated;
        let node = DiffNode::new(
            label,
            entries_status(&comparison.entries),
            DiffBody::Directory(DirectoryDiff {
                entries: comparison.entries,
                total_entries: comparison.total_entries,
                truncated,
            }),
        );
        Ok(complete_unless(node, truncated))
    }

    fn compare_zip_bytes(
        &self,
        label: &str,
        left: &[u8],
        right: &[u8],
        depth: usize,
        ctx: &mut Ctx,
    ) -> Result<DiffNode> {
        let left_stream = self.open_zip_stream(left, label, ctx)?;
        let right_stream = self.open_zip_stream(right, label, ctx)?;
        self.compare_stream_entries(label, &left_stream, &right_stream, "zip", depth, ctx)
    }

    fn open_zip_stream<'a>(
        &self,
        bytes: &'a [u8],
        label: &str,
        ctx: &mut Ctx,
    ) -> Result<ZipStream<'a>> {
        let mut archive = ZipArchive::new(std::io::Cursor::new(bytes))?;
        let total = archive.len();
        let limit = self.options.max_zip_entries;
        let mut entries = BTreeMap::new();
        if total > limit {
            ctx.omit();
        }
        for index in 0..total.min(limit) {
            let (name, size, compressed_size, is_lzma, is_dir) = {
                let raw = archive
                    .by_index_raw(index)
                    .map_err(|error| DiffError::Invalid(format!("zip entry #{index}: {error}")))?;
                (
                    raw.name().replace('\\', "/"),
                    raw.size(),
                    raw.compressed_size(),
                    matches!(raw.compression(), zip::CompressionMethod::Lzma),
                    raw.is_dir(),
                )
            };
            if is_dir {
                continue;
            }
            if size > self.options.max_file_bytes {
                ctx.warn(format!(
                    "{}: archive entry {name} is larger than the detailed-diff limit",
                    display_label(label)
                ));
                ctx.omit();
                continue;
            }
            let meta = ZipEntryMeta {
                index,
                size,
                compressed_size,
                is_lzma,
            };
            if entries.insert(name.clone(), meta).is_some() {
                ctx.warn(format!(
                    "{}: duplicate archive entry name {name}; keeping the last one",
                    display_label(label)
                ));
            }
        }
        if total > limit {
            ctx.warn(format!(
                "{}: archive has {total} entries; only the first {limit} were read",
                display_label(label)
            ));
        }
        Ok(ZipStream {
            archive: Mutex::new(archive),
            entries,
            charged: Mutex::new(BTreeSet::new()),
            stamps: Mutex::new(BTreeMap::new()),
            stopped: AtomicBool::new(false),
            max_file_bytes: self.options.max_file_bytes,
            label: label.to_string(),
        })
    }

    fn detect_stream_renames<S: StreamEntries>(
        &self,
        left: &S,
        right: &S,
        left_only: &[String],
        right_only: &[String],
        ctx: &mut Ctx,
    ) -> Result<BTreeMap<String, String>> {
        if left_only.is_empty() || right_only.is_empty() {
            return Ok(BTreeMap::new());
        }
        let mut right_by_size: BTreeMap<u64, Vec<&String>> = BTreeMap::new();
        for key in right_only {
            if let Some(size) = right.size(key) {
                right_by_size.entry(size).or_default().push(key);
            }
        }
        let mut buckets: BTreeMap<u64, BTreeMap<String, VecDeque<String>>> = BTreeMap::new();
        let mut loaded = BTreeSet::new();
        let mut pairs = BTreeMap::new();
        for left_key in left_only {
            let Some(left_size) = left.size(left_key) else {
                continue;
            };
            let Some(_) = right_by_size.get(&left_size) else {
                continue;
            };
            let Some(left_stamp) = left.stamp(left_key, ctx)? else {
                continue;
            };
            if !loaded.contains(&left_stamp.size) {
                let mut by_hash: BTreeMap<String, VecDeque<String>> = BTreeMap::new();
                if let Some(candidates) = right_by_size.get(&left_stamp.size) {
                    for candidate in candidates {
                        let Some(stamp) = right.stamp(candidate, ctx)? else {
                            continue;
                        };
                        if stamp.size == left_stamp.size {
                            by_hash
                                .entry(stamp.hash)
                                .or_default()
                                .push_back((*candidate).clone());
                        }
                    }
                }
                buckets.insert(left_stamp.size, by_hash);
                loaded.insert(left_stamp.size);
            }
            if let Some(queue) = buckets
                .get_mut(&left_stamp.size)
                .and_then(|by_hash| by_hash.get_mut(&left_stamp.hash))
                && let Some(candidate) = queue.pop_front()
            {
                pairs.insert(left_key.clone(), candidate);
            }
        }
        Ok(pairs)
    }

    fn compare_stream_entries<S: StreamEntries>(
        &self,
        label: &str,
        left: &S,
        right: &S,
        format: &str,
        depth: usize,
        ctx: &mut Ctx,
    ) -> Result<DiffNode> {
        let left_only: Vec<String> = left
            .names()
            .into_iter()
            .filter(|key| !right.contains(key))
            .map(str::to_string)
            .collect();
        let right_only: Vec<String> = right
            .names()
            .into_iter()
            .filter(|key| !left.contains(key))
            .map(str::to_string)
            .collect();
        let renames = self.detect_stream_renames(left, right, &left_only, &right_only, ctx)?;
        let renamed_right: BTreeSet<String> = renames.values().cloned().collect();
        let mut keys: Vec<String> = left
            .names()
            .into_iter()
            .chain(right.names())
            .filter(|key| !renamed_right.contains(*key))
            .map(str::to_string)
            .collect();
        keys.sort();
        keys.dedup();
        let total_entries = keys.len();
        let limit = self.options.max_zip_entries;
        let count_truncated = total_entries > limit;
        if count_truncated {
            ctx.warn(format!(
                "{}: archive entry limit ({limit}) reached; some entries were skipped",
                display_label(label)
            ));
            keys.truncate(limit);
        }
        let truncated = count_truncated;
        if let Some(progress) = &self.progress
            && progress.enabled()
        {
            progress.add_total(keys.len());
            progress.set_message(&format!("Comparing {}", display_label(label)));
        }
        let display = display_label(label).to_string();

        let mut nodes: BTreeMap<String, DiffNode> = BTreeMap::new();
        let mut pending: Vec<String> = Vec::new();
        for key in &keys {
            let right_key = renames.get(key).map(String::as_str).unwrap_or(key.as_str());
            let left_size = left.size(key);
            let right_size = right.size(right_key);
            match (left_size, right_size) {
                (Some(left_size), Some(right_size)) => {
                    // The central directory already tells us the unpacked
                    // sizes. Different sizes cannot be equal, so skip the
                    // first read and materialise them only for the detailed
                    // comparison below.
                    if left_size != right_size {
                        pending.push(key.clone());
                        continue;
                    }
                    let left_data = left.read(key, ctx)?;
                    let right_data = right.read(right_key, ctx)?;
                    match (left_data, right_data) {
                        (Some(left_data), Some(right_data)) => {
                            if left_data == right_data {
                                let node = self.compare_archive_pair(
                                    key,
                                    &left_data,
                                    &right_data,
                                    renames.get(key).map(String::as_str),
                                    depth,
                                    ctx,
                                )?;
                                nodes.insert(key.clone(), node);
                            } else {
                                pending.push(key.clone());
                            }
                        }
                        (Some(left_data), None) => {
                            let node = self.one_sided_archive_node(
                                key,
                                PresenceSide::LeftOnly,
                                &left_data,
                                ctx,
                            );
                            nodes.insert(key.clone(), node);
                        }
                        (None, Some(right_data)) => {
                            let node = self.one_sided_archive_node(
                                key,
                                PresenceSide::RightOnly,
                                &right_data,
                                ctx,
                            );
                            nodes.insert(key.clone(), node);
                        }
                        (None, None) => {}
                    }
                }
                (Some(_), None) => {
                    if let Some(stamp) = left.stamp(key, ctx)? {
                        nodes.insert(
                            key.clone(),
                            self.one_sided_archive_stamp(
                                key,
                                PresenceSide::LeftOnly,
                                stamp.kind,
                                stamp.size,
                                ctx,
                            ),
                        );
                    }
                }
                (None, Some(_)) => {
                    if let Some(stamp) = right.stamp(right_key, ctx)? {
                        nodes.insert(
                            key.clone(),
                            self.one_sided_archive_stamp(
                                key,
                                PresenceSide::RightOnly,
                                stamp.kind,
                                stamp.size,
                                ctx,
                            ),
                        );
                    }
                }
                (None, None) => {}
            }
            if let Some(progress) = &self.progress
                && progress.enabled()
                && nodes.contains_key(key)
            {
                progress.inc(&format!("{display}: {key}"));
            }
        }

        // Re-read changed pairs in bounded batches, then run the detailed
        // comparison in parallel. Payloads leave memory as soon as the batch
        // finishes, so peak usage follows the batch budget instead of the
        // archive size.
        const BATCH_BYTES: u64 = 32 * 1024 * 1024;
        let batch_limit = self.options.jobs.max(1).saturating_mul(2).max(2);
        let mut index = 0usize;
        while index < pending.len() {
            let mut batch: Vec<(String, String, Vec<u8>, Vec<u8>)> = Vec::new();
            let mut batch_bytes = 0u64;
            while index < pending.len() && batch.len() < batch_limit {
                let key = pending[index].clone();
                index += 1;
                let right_key = renames.get(&key).cloned().unwrap_or_else(|| key.clone());
                let left_data = left.read(&key, ctx)?;
                let right_data = right.read(&right_key, ctx)?;
                match (left_data, right_data) {
                    (Some(left_data), Some(right_data)) => {
                        batch_bytes = batch_bytes
                            .saturating_add(left_data.len() as u64)
                            .saturating_add(right_data.len() as u64);
                        batch.push((key, right_key, left_data, right_data));
                        if batch_bytes >= BATCH_BYTES {
                            break;
                        }
                    }
                    (Some(left_data), None) => {
                        let node = self.one_sided_archive_node(
                            &key,
                            PresenceSide::LeftOnly,
                            &left_data,
                            ctx,
                        );
                        nodes.insert(key, node);
                    }
                    (None, Some(right_data)) => {
                        let node = self.one_sided_archive_node(
                            &key,
                            PresenceSide::RightOnly,
                            &right_data,
                            ctx,
                        );
                        nodes.insert(key, node);
                    }
                    (None, None) => {}
                }
            }
            if batch.is_empty() {
                continue;
            }
            let process =
                |(key, _right_key, left_data, right_data): &(String, String, Vec<u8>, Vec<u8>)| {
                    let mut local = Ctx::child(ctx.excludes.clone(), ctx.budget.clone());
                    let node = self.compare_archive_pair(
                        key,
                        left_data,
                        right_data,
                        renames.get(key).map(String::as_str),
                        depth,
                        &mut local,
                    );
                    (key.clone(), node, local)
                };
            let results: Vec<(String, Result<DiffNode>, Ctx)> = if self.options.jobs == 1 {
                batch.iter().map(process).collect()
            } else {
                batch.par_iter().map(process).collect()
            };
            for (key, result, local) in results {
                ctx.merge(local);
                let node = result?;
                if let Some(progress) = &self.progress
                    && progress.enabled()
                {
                    progress.inc(&format!("{display}: {key}"));
                }
                nodes.insert(key, node);
            }
        }

        let entries: Vec<DiffNode> = keys
            .into_iter()
            .filter_map(|key| nodes.remove(&key))
            .collect();
        let node = DiffNode::new(
            label,
            entries_status(&entries),
            DiffBody::Archive(ArchiveDiff {
                format: format.to_string(),
                entries,
                total_entries,
                truncated,
            }),
        );
        Ok(complete_unless(node, truncated))
    }

    fn compare_7z_bytes(
        &self,
        label: &str,
        left: &[u8],
        right: &[u8],
        depth: usize,
        ctx: &mut Ctx,
    ) -> Result<DiffNode> {
        let left_entries = self.read_7z_entries(left, label, ctx)?;
        let right_entries = self.read_7z_entries(right, label, ctx)?;
        self.compare_archive_entries(label, &left_entries, &right_entries, "7z", depth, ctx)
    }

    fn read_7z_entries(
        &self,
        bytes: &[u8],
        label: &str,
        ctx: &mut Ctx,
    ) -> Result<BTreeMap<String, Vec<u8>>> {
        let cursor = std::io::Cursor::new(bytes);
        let mut reader = sevenz_rust::SevenZReader::new(
            cursor,
            bytes.len() as u64,
            sevenz_rust::Password::empty(),
        )
        .map_err(|error| DiffError::Invalid(format!("7z: {error}")))?;
        let mut entries = BTreeMap::new();
        let mut seen_entries = 0usize;
        reader
            .for_each_entries(|entry, reader| {
                if entry.is_directory() {
                    return Ok(true);
                }
                if seen_entries >= self.options.max_zip_entries {
                    ctx.warn(format!(
                        "{}: archive entry limit ({}) reached; some entries were skipped",
                        display_label(label),
                        self.options.max_zip_entries
                    ));
                    ctx.omit();
                    return Ok(false);
                }
                seen_entries += 1;
                let name = entry
                    .name()
                    .replace('\\', "/")
                    .trim_start_matches("./")
                    .trim_start_matches('/')
                    .to_string();
                if entry.size() > self.options.max_file_bytes {
                    ctx.warn(format!(
                        "{}: archive entry {name} is larger than the detailed-diff limit",
                        display_label(label)
                    ));
                    ctx.omit();
                    return Ok(true);
                }
                let mut data = Vec::new();
                reader
                    .take(self.options.max_file_bytes + 1)
                    .read_to_end(&mut data)?;
                if data.len() as u64 > self.options.max_file_bytes {
                    ctx.warn(format!(
                        "{}: archive entry {name} exceeds the detailed-diff limit",
                        display_label(label)
                    ));
                    ctx.omit();
                    return Ok(true);
                }
                if !ctx.charge_expanded(label, data.len()) {
                    return Ok(false);
                }
                entries.insert(name, data);
                Ok(true)
            })
            .map_err(|error| DiffError::Invalid(format!("7z: {error}")))?;
        Ok(entries)
    }

    fn compare_rpm_bytes(
        &self,
        label: &str,
        left: &[u8],
        right: &[u8],
        depth: usize,
        ctx: &mut Ctx,
    ) -> Result<DiffNode> {
        let left_entries = self.read_rpm_entries(left, label, ctx)?;
        let right_entries = self.read_rpm_entries(right, label, ctx)?;
        self.compare_archive_entries(label, &left_entries, &right_entries, "rpm", depth, ctx)
    }

    fn read_rpm_entries(
        &self,
        bytes: &[u8],
        label: &str,
        ctx: &mut Ctx,
    ) -> Result<BTreeMap<String, Vec<u8>>> {
        let input = std::io::Cursor::new(bytes.to_vec());
        let mut reader = rpm::PackageReader::parse(input)
            .map_err(|error| DiffError::Invalid(format!("rpm: {error}")))?;
        let mut entries = BTreeMap::new();
        let mut metadata = serde_json::Map::new();
        for (key, value) in [
            ("name", reader.metadata.get_name()),
            ("version", reader.metadata.get_version()),
            ("release", reader.metadata.get_release()),
            ("arch", reader.metadata.get_arch()),
        ] {
            if let Ok(value) = value {
                metadata.insert(
                    key.to_string(),
                    serde_json::Value::String(value.to_string()),
                );
            }
        }
        if !metadata.is_empty() {
            entries.insert(
                ".rpm-metadata.json".to_string(),
                serde_json::to_vec_pretty(&serde_json::Value::Object(metadata))
                    .map_err(|error| DiffError::Invalid(format!("rpm metadata: {error}")))?,
            );
        }
        while let Some(file) = reader
            .next_file()
            .map_err(|error| DiffError::Invalid(format!("rpm: {error}")))?
        {
            if !file.has_payload() {
                continue;
            }
            let name = file
                .metadata
                .path()
                .to_string_lossy()
                .replace('\\', "/")
                .trim_start_matches('/')
                .to_string();
            if name.is_empty() {
                continue;
            }
            if entries.len() >= self.options.max_zip_entries {
                ctx.warn(format!(
                    "{}: archive entry limit ({}) reached; some entries were skipped",
                    display_label(label),
                    self.options.max_zip_entries
                ));
                ctx.omit();
                break;
            }
            if file.metadata.size() as u64 > self.options.max_file_bytes {
                ctx.warn(format!(
                    "{}: archive entry {name} is larger than the detailed-diff limit",
                    display_label(label)
                ));
                ctx.omit();
                continue;
            }
            let mut data = Vec::new();
            file.take(self.options.max_file_bytes + 1)
                .read_to_end(&mut data)?;
            if data.len() as u64 > self.options.max_file_bytes {
                ctx.warn(format!(
                    "{}: archive entry {name} exceeds the detailed-diff limit",
                    display_label(label)
                ));
                ctx.omit();
                continue;
            }
            if !ctx.charge_expanded(label, data.len()) {
                break;
            }
            entries.insert(name, data);
        }
        Ok(entries)
    }

    fn compare_cab_bytes(
        &self,
        label: &str,
        left: &[u8],
        right: &[u8],
        depth: usize,
        ctx: &mut Ctx,
    ) -> Result<DiffNode> {
        let left_entries = self.read_cab_entries(left, label, ctx)?;
        let right_entries = self.read_cab_entries(right, label, ctx)?;
        self.compare_archive_entries(label, &left_entries, &right_entries, "cab", depth, ctx)
    }

    fn read_cab_entries(
        &self,
        bytes: &[u8],
        label: &str,
        ctx: &mut Ctx,
    ) -> Result<BTreeMap<String, Vec<u8>>> {
        let mut cabinet = cab::Cabinet::new(std::io::Cursor::new(bytes))
            .map_err(|error| DiffError::Invalid(format!("cab: {error}")))?;
        let mut names: Vec<String> = cabinet
            .folder_entries()
            .flat_map(|folder| {
                folder
                    .file_entries()
                    .map(|file| file.name().to_string())
                    .collect::<Vec<_>>()
            })
            .collect();
        if names.len() > self.options.max_zip_entries {
            ctx.warn(format!(
                "{}: cab entry limit reached ({} > {})",
                display_label(label),
                names.len(),
                self.options.max_zip_entries
            ));
            names.truncate(self.options.max_zip_entries);
            ctx.omit();
        }
        let mut entries = BTreeMap::new();
        for name in names {
            let mut reader = cabinet
                .read_file(&name)
                .map_err(|error| DiffError::Invalid(format!("cab {name}: {error}")))?;
            let mut data = Vec::new();
            read_bounded(&mut reader, self.options.max_file_bytes, &mut data)
                .map_err(|error| DiffError::Invalid(format!("cab {name}: {error}")))?;
            if data.len() as u64 > self.options.max_file_bytes {
                ctx.warn(format!(
                    "{}: archive entry {name} exceeds the detailed-diff limit",
                    display_label(label)
                ));
                ctx.omit();
                continue;
            }
            if !ctx.charge_expanded(label, data.len()) {
                break;
            }
            entries.insert(name, data);
        }
        Ok(entries)
    }

    fn compare_xar_bytes(
        &self,
        label: &str,
        left: &[u8],
        right: &[u8],
        depth: usize,
        ctx: &mut Ctx,
    ) -> Result<DiffNode> {
        let left_entries = self.read_xar_entries(left, label, ctx)?;
        let right_entries = self.read_xar_entries(right, label, ctx)?;
        self.compare_archive_entries(label, &left_entries, &right_entries, "xar", depth, ctx)
    }

    fn read_xar_entries(
        &self,
        bytes: &[u8],
        label: &str,
        ctx: &mut Ctx,
    ) -> Result<BTreeMap<String, Vec<u8>>> {
        if bytes.len() < 28 || !bytes.starts_with(b"xar!") {
            return Err(DiffError::Invalid("xar: truncated header".to_string()));
        }
        let header_size = usize::from(u16::from_be_bytes([bytes[4], bytes[5]]));
        if !(28..=bytes.len()).contains(&header_size) {
            return Err(DiffError::Invalid("xar: invalid header size".to_string()));
        }
        let toc_compressed = u64::from_be_bytes(
            bytes[8..16]
                .try_into()
                .map_err(|_| DiffError::Invalid("xar: truncated header".to_string()))?,
        ) as usize;
        let toc_uncompressed = u64::from_be_bytes(
            bytes[16..24]
                .try_into()
                .map_err(|_| DiffError::Invalid("xar: truncated header".to_string()))?,
        ) as usize;
        let toc_end = header_size
            .checked_add(toc_compressed)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| DiffError::Invalid("xar: truncated table of contents".to_string()))?;
        // The header's declared TOC size is untrusted: it bounds only the
        // pre-allocation (capped), while the actual decompressed bytes are
        // read through the per-file limit before the size is compared.
        if toc_uncompressed as u64 > self.options.max_file_bytes {
            return Err(DiffError::Invalid(format!(
                "xar: table of contents declares {toc_uncompressed} bytes, above the detailed-diff limit"
            )));
        }
        let mut toc = Vec::with_capacity(reserve_for_declared(toc_uncompressed as u64));
        read_bounded(
            flate2::read::ZlibDecoder::new(&bytes[header_size..toc_end]),
            self.options.max_file_bytes,
            &mut toc,
        )
        .map_err(|error| DiffError::Invalid(format!("xar toc: {error}")))?;
        if toc.len() != toc_uncompressed {
            return Err(DiffError::Invalid(format!(
                "xar: table of contents size mismatch ({} != {toc_uncompressed})",
                toc.len()
            )));
        }
        let xml = if ctx.charge_expanded(label, toc.len()) {
            String::from_utf8(toc).map_err(|_| {
                DiffError::Invalid("xar: table of contents is not UTF-8".to_string())
            })?
        } else {
            return Err(DiffError::Invalid(
                "xar: table of contents exceeds the expanded-bytes budget".to_string(),
            ));
        };
        let heap = &bytes[toc_end..];

        #[derive(Default)]
        struct Frame {
            name: String,
            kind: String,
            link: String,
            length: u64,
            offset: u64,
            size: u64,
            encoding: String,
        }

        let mut frames: Vec<Frame> = Vec::new();
        let mut directories: Vec<String> = Vec::new();
        let mut field: Option<String> = None;
        let mut text = String::new();
        let mut entries = BTreeMap::new();
        let mut truncated = false;
        let mut reader = quick_xml::Reader::from_str(&xml);
        reader.config_mut().trim_text(true);

        loop {
            match reader.read_event() {
                Ok(Event::Start(start)) => {
                    let name = start.local_name().as_ref().to_string();
                    match name.as_str() {
                        "file" => frames.push(Frame::default()),
                        "data" => {
                            if let Some(frame) = frames.last_mut() {
                                for attribute in start.attributes().with_checks(false) {
                                    let attribute = attribute.map_err(|error| {
                                        DiffError::Invalid(format!("xar toc: {error}"))
                                    })?;
                                    if attribute.key.local_name().as_ref() == "style" {
                                        frame.encoding = attribute
                                            .normalized_value(XmlVersion::Implicit1_0)
                                            .map_err(|error| {
                                                DiffError::Invalid(format!("xar toc: {error}"))
                                            })?
                                            .into_owned();
                                    }
                                }
                            }
                        }
                        "name" | "type" | "link" | "length" | "offset" | "size" => {
                            field = Some(name);
                            text.clear();
                        }
                        _ => {}
                    }
                }
                Ok(Event::Empty(empty)) => {
                    let name = empty.local_name().as_ref().to_string();
                    if name == "encoding"
                        && let Some(frame) = frames.last_mut()
                    {
                        for attribute in empty.attributes().with_checks(false) {
                            let attribute = attribute
                                .map_err(|error| DiffError::Invalid(format!("xar toc: {error}")))?;
                            if attribute.key.local_name().as_ref() == "style" {
                                frame.encoding = attribute
                                    .normalized_value(XmlVersion::Implicit1_0)
                                    .map_err(|error| {
                                        DiffError::Invalid(format!("xar toc: {error}"))
                                    })?
                                    .into_owned();
                            }
                        }
                    }
                }
                Ok(Event::Text(event)) if field.is_some() => {
                    text.push_str(&event.xml_content(XmlVersion::Implicit1_0));
                }
                Ok(Event::CData(event)) if field.is_some() => {
                    text.push_str(event.as_ref());
                }
                Ok(Event::End(end)) => {
                    let name = end.local_name().as_ref().to_string();
                    if let Some(frame) = frames.last_mut() {
                        match name.as_str() {
                            "name" => frame.name = text.clone(),
                            "type" => {
                                frame.kind = text.clone();
                                if frame.kind == "directory" {
                                    directories.push(frame.name.clone());
                                }
                            }
                            "link" => frame.link = text.clone(),
                            "length" => frame.length = text.trim().parse().unwrap_or(0),
                            "offset" => frame.offset = text.trim().parse().unwrap_or(0),
                            "size" => frame.size = text.trim().parse().unwrap_or(0),
                            _ => {}
                        }
                    }
                    if field.as_deref() == Some(name.as_str()) {
                        field = None;
                        text.clear();
                    }
                    if name == "file" {
                        let frame = frames.pop().ok_or_else(|| {
                            DiffError::Invalid("xar: unbalanced file".to_string())
                        })?;
                        if frame.kind == "directory" {
                            directories.pop();
                        } else if frame.kind == "symlink" {
                            if entries.len() >= self.options.max_zip_entries {
                                truncated = true;
                                ctx.omit();
                                break;
                            }
                            let mut path = directories.join("/");
                            if !path.is_empty() {
                                path.push('/');
                            }
                            path.push_str(&frame.name);
                            let data = format!("symlink -> {}", frame.link).into_bytes();
                            if !ctx.charge_expanded(label, data.len()) {
                                truncated = true;
                                break;
                            }
                            entries.insert(path, data);
                        } else if frame.kind.is_empty() || frame.kind == "file" {
                            if entries.len() >= self.options.max_zip_entries {
                                truncated = true;
                                ctx.omit();
                                break;
                            }
                            if frame.size > self.options.max_file_bytes {
                                ctx.warn(format!(
                                    "{}: xar entry {} exceeds the detailed-diff limit",
                                    display_label(label),
                                    frame.name
                                ));
                                ctx.omit();
                                continue;
                            }
                            let mut path = directories.join("/");
                            if !path.is_empty() {
                                path.push('/');
                            }
                            path.push_str(&frame.name);
                            let start = usize::try_from(frame.offset).map_err(|_| {
                                DiffError::Invalid("xar: offset overflow".to_string())
                            })?;
                            let end = start
                                .checked_add(usize::try_from(frame.length).map_err(|_| {
                                    DiffError::Invalid("xar: size overflow".to_string())
                                })?)
                                .ok_or_else(|| {
                                    DiffError::Invalid("xar: size overflow".to_string())
                                })?;
                            let raw = heap.get(start..end).ok_or_else(|| {
                                DiffError::Invalid("xar: truncated heap data".to_string())
                            })?;
                            let mut data = Vec::with_capacity(reserve_for_declared(frame.size));
                            if frame.encoding.contains("gzip") {
                                // XAR's "application/x-gzip" payloads are zlib
                                // streams (RFC 1950), matching libarchive/xar.
                                read_bounded(
                                    flate2::read::ZlibDecoder::new(raw),
                                    self.options.max_file_bytes,
                                    &mut data,
                                )
                                .map_err(|error| {
                                    DiffError::Invalid(format!("xar {path}: {error}"))
                                })?;
                            } else if frame.encoding.contains("bzip2") {
                                read_bounded(
                                    bzip2::read::BzDecoder::new(raw),
                                    self.options.max_file_bytes,
                                    &mut data,
                                )
                                .map_err(|error| {
                                    DiffError::Invalid(format!("xar {path}: {error}"))
                                })?;
                            } else if frame.encoding.contains("xz") {
                                read_bounded(
                                    liblzma::read::XzDecoder::new(raw),
                                    self.options.max_file_bytes,
                                    &mut data,
                                )
                                .map_err(|error| {
                                    DiffError::Invalid(format!("xar {path}: {error}"))
                                })?;
                            } else if frame.encoding.contains("lzma") {
                                // lzma_rs writes through `Write`, so the bound is
                                // enforced by the destination instead.
                                let mut output = LimitedBuffer::new(self.options.max_file_bytes);
                                lzma_rs::lzma_decompress(&mut &raw[..], &mut output).map_err(
                                    |error| DiffError::Invalid(format!("xar {path}: {error}")),
                                )?;
                                data = output.data;
                            } else {
                                data.extend_from_slice(raw);
                            }
                            if data.len() as u64 > self.options.max_file_bytes {
                                ctx.warn(format!(
                                    "{}: xar entry {} expands beyond the detailed-diff limit",
                                    display_label(label),
                                    frame.name
                                ));
                                ctx.omit();
                                continue;
                            }
                            if data.len() as u64 != frame.size {
                                return Err(DiffError::Invalid(format!(
                                    "xar {path}: decoded size mismatch ({} != {})",
                                    data.len(),
                                    frame.size
                                )));
                            }
                            if !ctx.charge_expanded(label, data.len()) {
                                truncated = true;
                                break;
                            }
                            entries.insert(path, data);
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(error) => {
                    return Err(DiffError::Invalid(format!("xar toc: {error}")));
                }
            }
        }

        if truncated {
            ctx.warn(format!(
                "{}: xar entry limit reached ({}); some entries were skipped",
                display_label(label),
                self.options.max_zip_entries
            ));
        }
        Ok(entries)
    }

    fn compare_cpio_bytes(
        &self,
        label: &str,
        left: &[u8],
        right: &[u8],
        depth: usize,
        ctx: &mut Ctx,
    ) -> Result<DiffNode> {
        let left_stream = self.open_cpio_stream(left, label, ctx)?;
        let right_stream = self.open_cpio_stream(right, label, ctx)?;
        self.compare_stream_entries(label, &left_stream, &right_stream, "cpio", depth, ctx)
    }

    fn open_cpio_stream<'a>(
        &self,
        bytes: &'a [u8],
        label: &str,
        ctx: &mut Ctx,
    ) -> Result<SliceStream<'a>> {
        let mut entries = BTreeMap::new();
        let mut offset = 0usize;
        while offset + 110 <= bytes.len() {
            let header = &bytes[offset..offset + 110];
            let magic = &header[0..6];
            if magic != b"070701" && magic != b"070702" {
                if header.iter().all(|byte| *byte == 0) {
                    break;
                }
                return Err(DiffError::Invalid("cpio: invalid newc header".into()));
            }
            let mode = cpio_field(header, 14)?;
            let filesize = cpio_field(header, 54)?;
            let namesize = cpio_field(header, 94)?;
            if namesize == 0 {
                return Err(DiffError::Invalid("cpio: empty entry name".into()));
            }
            offset += 110;
            if namesize > bytes.len() - offset {
                return Err(DiffError::Invalid("cpio: truncated entry name".into()));
            }
            let name_bytes = &bytes[offset..offset + namesize];
            let name_end = name_bytes
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(namesize);
            let name = String::from_utf8_lossy(&name_bytes[..name_end])
                .trim_start_matches("./")
                .trim_start_matches('/')
                .replace('\\', "/");
            offset = (offset + namesize + 3) & !3;
            if name == "TRAILER!!!" {
                break;
            }
            if filesize > bytes.len().saturating_sub(offset) {
                return Err(DiffError::Invalid("cpio: truncated entry data".into()));
            }
            let data_offset = offset;
            offset = (offset + filesize + 3) & !3;
            if mode & 0o170000 != 0o100000 {
                continue;
            }
            if entries.len() >= self.options.max_zip_entries {
                ctx.warn(format!(
                    "{}: archive entry limit ({}) reached; some entries were skipped",
                    display_label(label),
                    self.options.max_zip_entries
                ));
                ctx.omit();
                break;
            }
            if filesize as u64 > self.options.max_file_bytes {
                ctx.warn(format!(
                    "{}: archive entry {name} is larger than the detailed-diff limit",
                    display_label(label)
                ));
                ctx.omit();
                continue;
            }
            entries.insert(
                name,
                SliceEntryMeta {
                    data: SliceData::Range(data_offset, filesize),
                    size: filesize as u64,
                },
            );
        }
        Ok(SliceStream {
            bytes,
            entries,
            charged: Mutex::new(BTreeSet::new()),
            stamps: Mutex::new(BTreeMap::new()),
            stopped: AtomicBool::new(false),
            max_file_bytes: self.options.max_file_bytes,
            label: label.to_string(),
        })
    }

    fn compare_iso_bytes(
        &self,
        label: &str,
        left: &[u8],
        right: &[u8],
        depth: usize,
        ctx: &mut Ctx,
    ) -> Result<DiffNode> {
        let left_entries = self.read_iso_entries(left, label, ctx)?;
        let right_entries = self.read_iso_entries(right, label, ctx)?;
        self.compare_archive_entries(label, &left_entries, &right_entries, "iso", depth, ctx)
    }

    fn read_iso_entries(
        &self,
        bytes: &[u8],
        label: &str,
        ctx: &mut Ctx,
    ) -> Result<BTreeMap<String, Vec<u8>>> {
        let cursor = std::io::Cursor::new(bytes);
        let iso = iso9660::ISO9660::new(cursor)
            .map_err(|error| DiffError::Invalid(format!("iso: {error}")))?;
        let mut entries = BTreeMap::new();
        self.collect_iso_dir(&iso.root, "", &mut entries, label, ctx, 0)?;
        Ok(entries)
    }

    fn collect_iso_dir<T: iso9660::ISO9660Reader>(
        &self,
        dir: &iso9660::ISODirectory<T>,
        prefix: &str,
        entries: &mut BTreeMap<String, Vec<u8>>,
        label: &str,
        ctx: &mut Ctx,
        depth: usize,
    ) -> Result<()> {
        if depth > 32 {
            ctx.warn(format!(
                "{}: ISO directory recursion limit reached",
                display_label(label)
            ));
            return Ok(());
        }
        for entry in dir.contents() {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    ctx.warn(format!(
                        "{}: cannot read ISO directory entry: {error}",
                        display_label(label)
                    ));
                    continue;
                }
            };
            let name = entry.identifier();
            if name.is_empty() || name == "." || name == ".." {
                continue;
            }
            let path = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix}/{name}")
            };
            match entry {
                iso9660::DirectoryEntry::Directory(child) => {
                    self.collect_iso_dir(&child, &path, entries, label, ctx, depth + 1)?;
                }
                iso9660::DirectoryEntry::File(file) => {
                    if entries.len() >= self.options.max_zip_entries {
                        ctx.warn(format!(
                            "{}: archive entry limit ({}) reached; some entries were skipped",
                            display_label(label),
                            self.options.max_zip_entries
                        ));
                        ctx.omit();
                        return Ok(());
                    }
                    if file.size() as u64 > self.options.max_file_bytes {
                        ctx.warn(format!(
                            "{}: archive entry {path} is larger than the detailed-diff limit",
                            display_label(label)
                        ));
                        ctx.omit();
                        continue;
                    }
                    let mut data = Vec::new();
                    file.read()
                        .take(self.options.max_file_bytes + 1)
                        .read_to_end(&mut data)?;
                    if data.len() as u64 > self.options.max_file_bytes {
                        ctx.warn(format!(
                            "{}: archive entry {path} exceeds the detailed-diff limit",
                            display_label(label)
                        ));
                        ctx.omit();
                        continue;
                    }
                    if !ctx.charge_expanded(label, data.len()) {
                        return Ok(());
                    }
                    entries.insert(path, data);
                }
            }
        }
        Ok(())
    }

    fn compare_deb_bytes(
        &self,
        label: &str,
        left: &[u8],
        right: &[u8],
        depth: usize,
        ctx: &mut Ctx,
    ) -> Result<DiffNode> {
        let left_stream = self.open_ar_stream(left, label, ctx)?;
        let right_stream = self.open_ar_stream(right, label, ctx)?;
        self.compare_stream_entries(label, &left_stream, &right_stream, "deb", depth, ctx)
    }

    fn compare_ar_bytes(
        &self,
        label: &str,
        left: &[u8],
        right: &[u8],
        depth: usize,
        ctx: &mut Ctx,
    ) -> Result<DiffNode> {
        let left_stream = self.open_ar_stream(left, label, ctx)?;
        let right_stream = self.open_ar_stream(right, label, ctx)?;
        self.compare_stream_entries(label, &left_stream, &right_stream, "ar", depth, ctx)
    }

    fn open_ar_stream<'a>(
        &self,
        bytes: &'a [u8],
        label: &str,
        ctx: &mut Ctx,
    ) -> Result<SliceStream<'a>> {
        const MAGIC: &[u8] = b"!<arch>\n";
        const HEADER_LEN: usize = 60;
        if !bytes.starts_with(MAGIC) {
            return Err(DiffError::Invalid("ar: invalid magic".into()));
        }

        let mut entries = BTreeMap::new();
        let mut long_names: Option<Vec<u8>> = None;
        let mut offset = MAGIC.len();
        while offset < bytes.len() {
            if bytes.len() - offset < HEADER_LEN {
                return Err(DiffError::Invalid("ar: truncated member header".into()));
            }
            let header = &bytes[offset..offset + HEADER_LEN];
            if &header[58..60] != b"`\n" {
                return Err(DiffError::Invalid("ar: invalid member header".into()));
            }
            let raw_name = String::from_utf8_lossy(&header[0..16]);
            let raw_name = raw_name.trim_end();
            let size = std::str::from_utf8(&header[48..58])
                .map_err(|_| DiffError::Invalid("ar: invalid member size".into()))?
                .trim()
                .parse::<usize>()
                .map_err(|_| DiffError::Invalid("ar: invalid member size".into()))?;
            offset += HEADER_LEN;
            if size > bytes.len() - offset {
                return Err(DiffError::Invalid("ar: member exceeds file size".into()));
            }
            let data_offset = offset;
            let data = &bytes[offset..offset + size];
            offset += size + (size & 1);

            if raw_name == "//" {
                long_names = Some(data.to_vec());
                continue;
            }
            let name = if let Some(index) = raw_name
                .strip_prefix('/')
                .and_then(|rest| rest.parse::<usize>().ok())
            {
                let table = long_names
                    .as_deref()
                    .ok_or_else(|| DiffError::Invalid("ar: long name without name table".into()))?;
                let name_start = index;
                if name_start >= table.len() {
                    return Err(DiffError::Invalid("ar: long name out of range".into()));
                }
                let name_end = table[name_start..]
                    .iter()
                    .position(|byte| *byte == b'\n' || *byte == 0)
                    .map(|position| name_start + position)
                    .unwrap_or(table.len());
                String::from_utf8_lossy(&table[name_start..name_end])
                    .trim_end_matches('/')
                    .to_string()
            } else {
                raw_name.trim_end_matches('/').to_string()
            };
            let (name, data_offset, data_len) = if let Some(length) = raw_name
                .strip_prefix("#1/")
                .and_then(|rest| rest.trim().parse::<usize>().ok())
            {
                if length > data.len() {
                    return Err(DiffError::Invalid(
                        "ar: BSD name exceeds member size".into(),
                    ));
                }
                let name = String::from_utf8_lossy(&data[..length])
                    .trim_end_matches('\0')
                    .to_string();
                (name, data_offset + length, data.len() - length)
            } else {
                (name, data_offset, data.len())
            };
            if name.is_empty() || name == "/" || name.starts_with("__.SYMDEF") {
                continue;
            }
            if entries.len() >= self.options.max_zip_entries {
                ctx.warn(format!(
                    "{}: archive entry limit ({}) reached; some entries were skipped",
                    display_label(label),
                    self.options.max_zip_entries
                ));
                ctx.omit();
                break;
            }
            if data_len as u64 > self.options.max_file_bytes {
                ctx.warn(format!(
                    "{}: archive entry {name} is larger than the detailed-diff limit",
                    display_label(label)
                ));
                ctx.omit();
                continue;
            }
            entries.insert(
                name,
                SliceEntryMeta {
                    data: SliceData::Range(data_offset, data_len),
                    size: data_len as u64,
                },
            );
        }
        Ok(SliceStream {
            bytes,
            entries,
            charged: Mutex::new(BTreeSet::new()),
            stamps: Mutex::new(BTreeMap::new()),
            stopped: AtomicBool::new(false),
            max_file_bytes: self.options.max_file_bytes,
            label: label.to_string(),
        })
    }

    fn compare_tar_bytes(
        &self,
        label: &str,
        left: &[u8],
        right: &[u8],
        depth: usize,
        ctx: &mut Ctx,
    ) -> Result<DiffNode> {
        let format = if left.starts_with(&[0x1f, 0x8b]) {
            "tar.gz"
        } else if left.starts_with(b"BZh") {
            "tar.bz2"
        } else if left.starts_with(&[0xFD, b'7', b'z', b'X', b'Z', 0]) {
            "tar.xz"
        } else if left.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
            "tar.zst"
        } else if left.starts_with(&[0x04, 0x22, 0x4D, 0x18]) {
            "tar.lz4"
        } else {
            "tar"
        };
        let right_is_compressed = right.starts_with(&[0x1f, 0x8b])
            || right.starts_with(b"BZh")
            || right.starts_with(&[0xFD, b'7', b'z', b'X', b'Z', 0])
            || right.starts_with(&[0x28, 0xB5, 0x2F, 0xFD])
            || right.starts_with(&[0x04, 0x22, 0x4D, 0x18]);
        if format == "tar" && !right_is_compressed {
            let left_stream = self.open_tar_stream(left, label, ctx)?;
            let right_stream = self.open_tar_stream(right, label, ctx)?;
            return self.compare_stream_entries(
                label,
                &left_stream,
                &right_stream,
                format,
                depth,
                ctx,
            );
        }
        let left_entries = self.read_tar_entries(left, label, ctx)?;
        let right_entries = self.read_tar_entries(right, label, ctx)?;
        self.compare_archive_entries(label, &left_entries, &right_entries, format, depth, ctx)
    }

    fn open_tar_stream<'a>(
        &self,
        bytes: &'a [u8],
        label: &str,
        ctx: &mut Ctx,
    ) -> Result<SliceStream<'a>> {
        let mut archive = tar::Archive::new(std::io::Cursor::new(bytes));
        let mut entries = BTreeMap::new();
        let mut count = 0usize;
        for entry in archive.entries()? {
            let mut entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    ctx.warn(format!(
                        "{}: cannot read tar entry: {error}",
                        display_label(label)
                    ));
                    continue;
                }
            };
            let entry_type = entry.header().entry_type();
            if !entry_type.is_file() && !entry_type.is_symlink() && !entry_type.is_hard_link() {
                continue;
            }
            if count >= self.options.max_zip_entries {
                ctx.warn(format!(
                    "{}: archive entry limit ({}) reached; some entries were skipped",
                    display_label(label),
                    self.options.max_zip_entries
                ));
                ctx.omit();
                break;
            }
            count += 1;
            let name = match entry.path() {
                Ok(path) => path
                    .to_string_lossy()
                    .replace('\\', "/")
                    .trim_start_matches("./")
                    .trim_start_matches('/')
                    .to_string(),
                Err(error) => {
                    ctx.warn(format!(
                        "{}: invalid tar entry path: {error}",
                        display_label(label)
                    ));
                    continue;
                }
            };
            let size = entry.header().size().unwrap_or(0);
            if size > self.options.max_file_bytes {
                ctx.warn(format!(
                    "{}: archive entry {name} is larger than the detailed-diff limit",
                    display_label(label)
                ));
                ctx.omit();
                continue;
            }
            if entry_type.is_gnu_sparse() {
                // Sparse payload bytes are not a contiguous range; materialise
                // only this entry rather than returning a wrong slice.
                let mut data = Vec::new();
                entry.read_to_end(&mut data).map_err(|error| {
                    DiffError::Invalid(format!("tar sparse entry {name}: {error}"))
                })?;
                if data.len() as u64 > self.options.max_file_bytes {
                    ctx.warn(format!(
                        "{}: archive entry {name} exceeds the detailed-diff limit",
                        display_label(label)
                    ));
                    ctx.omit();
                    continue;
                }
                entries.insert(
                    name,
                    SliceEntryMeta {
                        size: data.len() as u64,
                        data: SliceData::Owned(data),
                    },
                );
                continue;
            }
            if entry_type.is_symlink() || entry_type.is_hard_link() {
                let target = entry
                    .link_name()
                    .ok()
                    .flatten()
                    .map(|target| target.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default();
                let kind = if entry_type.is_symlink() {
                    "symlink"
                } else {
                    "hardlink"
                };
                let link = format!("{kind} -> {target}").into_bytes();
                entries.insert(
                    name,
                    SliceEntryMeta {
                        data: SliceData::Owned(link.clone()),
                        size: link.len() as u64,
                    },
                );
                continue;
            }
            let offset = entry.raw_file_position() as usize;
            let size = usize::try_from(size).map_err(|_| {
                DiffError::Invalid("tar: entry payload size does not fit in memory".into())
            })?;
            let end = offset
                .checked_add(size)
                .ok_or_else(|| DiffError::Invalid("tar: entry payload offset overflow".into()))?;
            if end > bytes.len() {
                return Err(DiffError::Invalid(
                    "tar: entry payload exceeds archive size".into(),
                ));
            }
            entries.insert(
                name,
                SliceEntryMeta {
                    data: SliceData::Range(offset, size),
                    size: size as u64,
                },
            );
        }
        Ok(SliceStream {
            bytes,
            entries,
            charged: Mutex::new(BTreeSet::new()),
            stamps: Mutex::new(BTreeMap::new()),
            stopped: AtomicBool::new(false),
            max_file_bytes: self.options.max_file_bytes,
            label: label.to_string(),
        })
    }

    fn compare_archive_pair(
        &self,
        key: &str,
        left_data: &[u8],
        right_data: &[u8],
        renamed_from: Option<&str>,
        depth: usize,
        ctx: &mut Ctx,
    ) -> Result<DiffNode> {
        let left_kind = detect_bytes(key, left_data);
        let right_kind = detect_bytes(key, right_data);
        let mut node = self.compare_bytes_internal(
            key,
            key,
            left_data,
            right_data,
            left_kind,
            right_kind,
            depth + 1,
            ctx,
        )?;
        if let Some(options) = &self.options.decompile
            && left_kind == FileKind::Executable
            && right_kind == FileKind::Executable
        {
            let mut disassembly = crate::disassembly::DisassemblyOptions::new(
                options.objdump.clone(),
                self.options.context,
                self.options.max_hunks,
            );
            disassembly.max_lines = options.max_lines;
            match crate::disassembly::compare_bytes(left_data, right_data, &disassembly) {
                Ok(decompiled) => node = node.with_decompiled(decompiled),
                Err(error) => ctx.warn(format!("{key}: decompilation failed ({error})")),
            }
        }
        if let Some(renamed) = renamed_from {
            node.label = renamed.to_string();
            node.renamed_from = Some(key.to_string());
            node.status = DiffStatus::Different;
        }
        ctx.summary.files_compared += 1;
        if node.status == DiffStatus::Equal {
            ctx.summary.files_equal += 1;
        } else {
            ctx.summary.files_changed += 1;
        }
        Ok(node)
    }

    fn one_sided_archive_node(
        &self,
        key: &str,
        side: PresenceSide,
        data: &[u8],
        ctx: &mut Ctx,
    ) -> DiffNode {
        self.one_sided_archive_stamp(key, side, detect_bytes(key, data), data.len() as u64, ctx)
    }

    fn one_sided_archive_stamp(
        &self,
        key: &str,
        side: PresenceSide,
        kind: FileKind,
        size: u64,
        ctx: &mut Ctx,
    ) -> DiffNode {
        match side {
            PresenceSide::LeftOnly => ctx.summary.files_removed += 1,
            PresenceSide::RightOnly => ctx.summary.files_added += 1,
        }
        DiffNode::new(
            key,
            DiffStatus::Different,
            DiffBody::OneSided(OneSidedInfo { side, kind, size }),
        )
    }

    fn compare_archive_entries(
        &self,
        label: &str,
        left_entries: &BTreeMap<String, Vec<u8>>,
        right_entries: &BTreeMap<String, Vec<u8>>,
        format: &str,
        depth: usize,
        ctx: &mut Ctx,
    ) -> Result<DiffNode> {
        let left_only: Vec<String> = left_entries
            .keys()
            .filter(|key| !right_entries.contains_key(*key))
            .cloned()
            .collect();
        let right_only: Vec<String> = right_entries
            .keys()
            .filter(|key| !left_entries.contains_key(*key))
            .cloned()
            .collect();
        let renames = detect_archive_renames(&left_only, &right_only, left_entries, right_entries);
        let renamed_right: BTreeSet<String> = renames.values().cloned().collect();
        let keys: Vec<String> = merged_keys(left_entries, right_entries)
            .into_iter()
            .filter(|key| !renamed_right.contains(key))
            .collect();
        let comparison = self.compare_entry_sets(
            label,
            keys,
            self.options.max_zip_entries,
            "archive",
            ctx,
            |key, local| {
                let right_data = right_entries.get(key).or_else(|| {
                    renames
                        .get(key)
                        .and_then(|renamed| right_entries.get(renamed))
                });
                match (left_entries.get(key), right_data) {
                    (Some(left_data), Some(right_data)) => self.compare_archive_pair(
                        key,
                        left_data,
                        right_data,
                        renames.get(key).map(String::as_str),
                        depth,
                        local,
                    ),
                    (Some(left_data), None) => Ok(self.one_sided_archive_node(
                        key,
                        PresenceSide::LeftOnly,
                        left_data,
                        local,
                    )),
                    (None, Some(right_data)) => Ok(self.one_sided_archive_node(
                        key,
                        PresenceSide::RightOnly,
                        right_data,
                        local,
                    )),
                    (None, None) => Ok(DiffNode::new(
                        key,
                        DiffStatus::Equal,
                        DiffBody::Equal(EqualInfo {
                            kind: FileKind::Unknown,
                            note: "missing on both sides".to_string(),
                        }),
                    )),
                }
            },
        )?;

        let node = DiffNode::new(
            label,
            entries_status(&comparison.entries),
            DiffBody::Archive(ArchiveDiff {
                format: format.to_string(),
                entries: comparison.entries,
                total_entries: comparison.total_entries,
                truncated: comparison.truncated,
            }),
        );
        Ok(complete_unless(node, comparison.truncated))
    }

    fn decompress_gzip(&self, bytes: &[u8], label: &str, ctx: &mut Ctx) -> Option<Vec<u8>> {
        let decoder = flate2::read::GzDecoder::new(std::io::Cursor::new(bytes));
        self.read_decompressed(decoder, label, "gzip", ctx)
    }

    fn decompress_zstd(&self, bytes: &[u8], label: &str, ctx: &mut Ctx) -> Option<Vec<u8>> {
        let decoder = match zstd::stream::read::Decoder::new(std::io::Cursor::new(bytes)) {
            Ok(decoder) => decoder,
            Err(error) => {
                ctx.warn(format!(
                    "{}: could not decompress zstd ({error}); falling back to binary",
                    display_label(label)
                ));
                return None;
            }
        };
        self.read_decompressed(decoder, label, "zstd", ctx)
    }

    fn decompress_lz4(&self, bytes: &[u8], label: &str, ctx: &mut Ctx) -> Option<Vec<u8>> {
        let decoder = lz4_flex::frame::FrameDecoder::new(std::io::Cursor::new(bytes));
        self.read_decompressed(decoder, label, "lz4", ctx)
    }

    fn decompress_bzip2(&self, bytes: &[u8], label: &str, ctx: &mut Ctx) -> Option<Vec<u8>> {
        let decoder = bzip2::read::BzDecoder::new(std::io::Cursor::new(bytes));
        self.read_decompressed(decoder, label, "bzip2", ctx)
    }

    fn decompress_xz(&self, bytes: &[u8], label: &str, ctx: &mut Ctx) -> Option<Vec<u8>> {
        let decoder = liblzma::read::XzDecoder::new(std::io::Cursor::new(bytes));
        self.read_decompressed(decoder, label, "xz", ctx)
    }

    fn read_decompressed<R: Read>(
        &self,
        decoder: R,
        label: &str,
        format: &str,
        ctx: &mut Ctx,
    ) -> Option<Vec<u8>> {
        let mut data = Vec::new();
        let mut limited = decoder.take(self.options.max_file_bytes + 1);
        if let Err(error) = limited.read_to_end(&mut data) {
            ctx.warn(format!(
                "{}: could not decompress {format} ({error}); falling back to binary",
                display_label(label)
            ));
            return None;
        }
        if data.len() as u64 > self.options.max_file_bytes {
            ctx.warn(format!(
                "{}: decompressed {format} exceeds the detailed-diff limit",
                display_label(label)
            ));
            return None;
        }
        if !ctx.charge_expanded(label, data.len()) {
            return None;
        }
        Some(data)
    }

    fn read_tar_entries(
        &self,
        bytes: &[u8],
        label: &str,
        ctx: &mut Ctx,
    ) -> Result<BTreeMap<String, Vec<u8>>> {
        let reader: Box<dyn Read> = if bytes.starts_with(&[0x1f, 0x8b]) {
            Box::new(flate2::read::GzDecoder::new(std::io::Cursor::new(bytes)))
        } else if bytes.starts_with(b"BZh") {
            Box::new(bzip2::read::BzDecoder::new(std::io::Cursor::new(bytes)))
        } else if bytes.starts_with(&[0xFD, b'7', b'z', b'X', b'Z', 0]) {
            Box::new(liblzma::read::XzDecoder::new(std::io::Cursor::new(bytes)))
        } else if bytes.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
            Box::new(
                zstd::stream::read::Decoder::new(std::io::Cursor::new(bytes))
                    .map_err(|error| DiffError::Invalid(format!("tar.zst: {error}")))?,
            )
        } else if bytes.starts_with(&[0x04, 0x22, 0x4D, 0x18]) {
            Box::new(lz4_flex::frame::FrameDecoder::new(std::io::Cursor::new(
                bytes,
            )))
        } else {
            Box::new(std::io::Cursor::new(bytes))
        };
        let mut archive = tar::Archive::new(reader);
        let mut entries = BTreeMap::new();
        let mut count = 0usize;

        for entry in archive.entries()? {
            let mut entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    ctx.warn(format!(
                        "{}: cannot read tar entry: {err}",
                        display_label(label)
                    ));
                    continue;
                }
            };
            let entry_type = entry.header().entry_type();
            if !entry_type.is_file() && !entry_type.is_symlink() && !entry_type.is_hard_link() {
                continue;
            }
            if count >= self.options.max_zip_entries {
                ctx.warn(format!(
                    "{}: archive entry limit ({}) reached; some entries were skipped",
                    display_label(label),
                    self.options.max_zip_entries
                ));
                ctx.omit();
                break;
            }
            count += 1;

            let name = match entry.path() {
                Ok(path) => path
                    .to_string_lossy()
                    .replace('\\', "/")
                    .trim_start_matches("./")
                    .trim_start_matches('/')
                    .to_string(),
                Err(err) => {
                    ctx.warn(format!(
                        "{}: invalid tar entry path: {err}",
                        display_label(label)
                    ));
                    continue;
                }
            };
            let size = entry.header().size().unwrap_or(0);
            if size > self.options.max_file_bytes {
                ctx.warn(format!(
                    "{}: archive entry {name} is larger than the detailed-diff limit",
                    display_label(label)
                ));
                ctx.omit();
                continue;
            }

            if entry_type.is_symlink() || entry_type.is_hard_link() {
                let target = entry
                    .link_name()
                    .ok()
                    .flatten()
                    .map(|target| target.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default();
                let kind = if entry_type.is_symlink() {
                    "symlink"
                } else {
                    "hardlink"
                };
                let link = format!("{kind} -> {target}").into_bytes();
                if !ctx.charge_expanded(label, link.len()) {
                    break;
                }
                entries.insert(name, link);
                continue;
            }

            let mut data = Vec::new();
            let mut limited = (&mut entry).take(self.options.max_file_bytes + 1);
            if let Err(err) = limited.read_to_end(&mut data) {
                ctx.warn(format!(
                    "{}: cannot read tar entry {name}: {err}",
                    display_label(label)
                ));
                continue;
            }
            if data.len() as u64 > self.options.max_file_bytes {
                ctx.warn(format!(
                    "{}: archive entry {name} exceeds the detailed-diff limit",
                    display_label(label)
                ));
                ctx.omit();
                continue;
            }
            if !ctx.charge_expanded(label, data.len()) {
                break;
            }
            entries.insert(name, data);
        }

        Ok(entries)
    }

    fn build_excludes(&self, ctx: &mut Ctx) -> Option<GlobSet> {
        if self.options.excludes.is_empty() {
            return None;
        }
        let mut builder = GlobSetBuilder::new();
        for pattern in &self.options.excludes {
            match Glob::new(pattern) {
                Ok(glob) => {
                    builder.add(glob);
                }
                Err(error) => ctx.warn(format!("invalid exclude glob {pattern:?}: {error}")),
            }
        }
        builder.build().ok()
    }

    fn collect_files(&self, root: &Path, ctx: &mut Ctx) -> Result<WalkedFiles> {
        let mut walked = WalkedFiles::default();
        // The walk is sorted so that a scan stopped at the entry limit picks
        // the same entries on every platform and every run; without the sort
        // the selection would follow the filesystem's readdir order.
        let walker = walkdir::WalkDir::new(root)
            .follow_links(false)
            .sort_by_file_name()
            .max_depth(self.options.max_depth);

        for entry in walker {
            match entry {
                Ok(entry) => {
                    let is_dir = entry.file_type().is_dir();
                    if is_dir {
                        // walkdir does not descend into a directory that sits
                        // at the depth limit, so its contents are never seen.
                        if entry.depth() > 0
                            && entry.depth() >= self.options.max_depth
                            && let Some(key) = self.relative_key(entry.path(), root, ctx)
                        {
                            walked.unscanned.push(key);
                        }
                        continue;
                    }
                    if !entry.file_type().is_file() {
                        continue;
                    }
                    if let Some(key) = self.relative_key(entry.path(), root, ctx) {
                        // Check before inserting: a tree with exactly
                        // `max_dir_entries` files is complete, so the scan may
                        // only report truncation after seeing one more
                        // comparable file. The walk still stops at that file
                        // instead of enumerating the rest.
                        if walked.files.len() >= self.options.max_dir_entries {
                            walked.scan_truncated = true;
                            break;
                        }
                        walked.files.insert(key, entry.path().to_path_buf());
                    }
                }
                Err(err) => ctx.warn(format!("{}: directory walk error: {err}", root.display())),
            }
        }

        Ok(walked)
    }

    /// Normalizes a walked path to the `/`-separated key used in reports and
    /// applies the exclude patterns; `None` means the entry is excluded.
    fn relative_key(&self, path: &Path, root: &Path, ctx: &Ctx) -> Option<String> {
        let relative = path.strip_prefix(root).ok()?;
        let relative = relative.to_string_lossy();
        let key = if relative.contains('\\') {
            relative.replace('\\', "/")
        } else {
            relative.into_owned()
        };
        if let Some(excludes) = &ctx.excludes
            && excludes.is_match(key.as_str())
        {
            return None;
        }
        Some(key)
    }
}

/// One parsed comparison: everything a format parser needs to compare two
/// sides whose bytes are already materialized.
struct CompareArgs<'a> {
    pub label: &'a str,
    pub hint: &'a str,
    pub left: &'a [u8],
    pub right: &'a [u8],
    pub left_kind: FileKind,
    pub right_kind: FileKind,
    pub kind: FileKind,
    pub depth: usize,
}

/// A format parser registered with the engine's dispatcher.
///
/// Each parser handles one or more `FileKind`s. Adding a format means
/// implementing this trait and registering the parser in `parser_registry`;
/// the dispatcher, the depth handling and the shared resource accounting
/// (`Ctx::charge_expanded`, `Ctx::omit`) stay untouched.
trait FormatParser: Sync + Send {
    /// The file kinds dispatched to this parser.
    fn kinds(&self) -> &'static [FileKind];

    /// Parser name, used by tests to pin the dispatch table.
    #[allow(dead_code)]
    fn name(&self) -> &'static str;

    /// Compares the two sides. Implementations must charge any data they
    /// unpack through `ctx.charge_expanded` and record skipped entries through
    /// `ctx.omit`, so the shared resource budget and the completeness model
    /// stay accurate.
    fn compare_bytes(
        &self,
        engine: &DiffEngine,
        args: CompareArgs<'_>,
        ctx: &mut Ctx,
    ) -> Result<DiffNode>;
}

struct TextParser;

impl FormatParser for TextParser {
    fn kinds(&self) -> &'static [FileKind] {
        &[FileKind::Text]
    }

    fn name(&self) -> &'static str {
        "text"
    }

    fn compare_bytes(
        &self,
        engine: &DiffEngine,
        args: CompareArgs<'_>,
        ctx: &mut Ctx,
    ) -> Result<DiffNode> {
        let (label, hint) = (args.label, args.hint);
        let has_syntax = tokens::language_for_name(hint).is_some();
        if has_syntax
            && matches!(engine.options.mode, DiffMode::Tree | DiffMode::Graph)
            && let Some(node) = engine.compare_tree_bytes(label, hint, args.left, args.right, ctx)
        {
            return Ok(node);
        }
        let token_mode = engine.options.mode == DiffMode::Token
            || (engine.options.mode == DiffMode::Text
                && engine.options.ignore_comments
                && has_syntax);
        if token_mode
            && let Some(node) = engine.compare_token_bytes(label, hint, args.left, args.right, ctx)
        {
            return Ok(node);
        }
        if engine.options.ignore_comments && !has_syntax {
            ctx.warn(format!(
                "{}: --ignore-comments is only supported for known source languages; using text diff",
                display_label(label)
            ));
        } else if engine.options.mode != DiffMode::Text && !has_syntax {
            ctx.warn(format!(
                "{}: no tree-sitter parser for this file type; using text diff",
                display_label(label)
            ));
        }
        Ok(engine.compare_text_bytes(label, args.left, args.right, "text", FileKind::Text, ctx))
    }
}

struct StructuredParser;

impl FormatParser for StructuredParser {
    fn kinds(&self) -> &'static [FileKind] {
        &[
            FileKind::Json,
            FileKind::Yaml,
            FileKind::Toml,
            FileKind::Ini,
            FileKind::Xml,
            FileKind::Html,
            FileKind::Plist,
            FileKind::Pickle,
            FileKind::Mo,
            FileKind::Wasm,
            FileKind::JavaClass,
            FileKind::AndroidXml,
            FileKind::GitIndex,
            FileKind::Ico,
            FileKind::SshPublicKey,
            FileKind::Structured,
            FileKind::Cbor,
            FileKind::MessagePack,
            FileKind::Protobuf,
            FileKind::Parquet,
            FileKind::Avro,
            FileKind::Orc,
            FileKind::Rtf,
            FileKind::Eml,
            FileKind::Epub,
            FileKind::Squashfs,
            FileKind::Snap,
            FileKind::AppImage,
            FileKind::Flatpak,
            FileKind::Dmg,
            FileKind::Msix,
            FileKind::Aab,
            FileKind::Ext4,
        ]
    }

    fn name(&self) -> &'static str {
        "structured"
    }

    fn compare_bytes(
        &self,
        engine: &DiffEngine,
        args: CompareArgs<'_>,
        ctx: &mut Ctx,
    ) -> Result<DiffNode> {
        Ok(engine.compare_structured_bytes(
            args.label,
            args.hint,
            args.left,
            args.right,
            args.left_kind,
            args.right_kind,
            ctx,
        ))
    }
}

/// One parser per leaf format whose comparison is a single call.
macro_rules! leaf_parser {
    ($struct_name:ident, $kind:ident, $parser_name:literal, $call:ident) => {
        struct $struct_name;

        impl FormatParser for $struct_name {
            fn kinds(&self) -> &'static [FileKind] {
                &[FileKind::$kind]
            }

            fn name(&self) -> &'static str {
                $parser_name
            }

            fn compare_bytes(
                &self,
                engine: &DiffEngine,
                args: CompareArgs<'_>,
                ctx: &mut Ctx,
            ) -> Result<DiffNode> {
                Ok(engine.$call(args.label, args.left, args.right, ctx))
            }
        }
    };
}

leaf_parser!(NotebookParser, Notebook, "notebook", compare_notebook_bytes);
leaf_parser!(SqliteParser, Sqlite, "sqlite", compare_sqlite_bytes);
leaf_parser!(
    ExecutableParser,
    Executable,
    "executable",
    compare_executable_bytes
);
leaf_parser!(DexParser, Dex, "dex", compare_dex_bytes);
leaf_parser!(FontParser, Font, "font", compare_font_bytes);
leaf_parser!(ImageParser, Image, "image", compare_image_bytes);

struct TableParser;

impl FormatParser for TableParser {
    fn kinds(&self) -> &'static [FileKind] {
        &[FileKind::Csv]
    }

    fn name(&self) -> &'static str {
        "table"
    }

    fn compare_bytes(
        &self,
        engine: &DiffEngine,
        args: CompareArgs<'_>,
        ctx: &mut Ctx,
    ) -> Result<DiffNode> {
        Ok(engine.compare_table_bytes(args.label, args.left, args.right, args.hint, ctx))
    }
}

struct DocumentParser;

impl FormatParser for DocumentParser {
    fn kinds(&self) -> &'static [FileKind] {
        &[
            FileKind::Pdf,
            FileKind::Ofd,
            FileKind::Docx,
            FileKind::Pptx,
            FileKind::Odt,
            FileKind::Odp,
            FileKind::DocumentCross,
        ]
    }

    fn name(&self) -> &'static str {
        "document"
    }

    fn compare_bytes(
        &self,
        engine: &DiffEngine,
        args: CompareArgs<'_>,
        ctx: &mut Ctx,
    ) -> Result<DiffNode> {
        Ok(engine.compare_document_bytes(
            args.label,
            args.left,
            args.right,
            args.kind,
            args.left_kind,
            args.right_kind,
            ctx,
        ))
    }
}

struct WorkbookParser;

impl FormatParser for WorkbookParser {
    fn kinds(&self) -> &'static [FileKind] {
        &[FileKind::Xlsx]
    }

    fn name(&self) -> &'static str {
        "workbook"
    }

    fn compare_bytes(
        &self,
        engine: &DiffEngine,
        args: CompareArgs<'_>,
        ctx: &mut Ctx,
    ) -> Result<DiffNode> {
        Ok(engine.compare_workbook_bytes(args.label, args.left, args.right, args.hint, ctx))
    }
}

/// Nested compressed streams: unpack both sides (charging the budget) and
/// recurse one level deeper with the restored file name hint.
struct CompressionParser;

impl FormatParser for CompressionParser {
    fn kinds(&self) -> &'static [FileKind] {
        &[
            FileKind::Gzip,
            FileKind::Zstd,
            FileKind::Bzip2,
            FileKind::Xz,
            FileKind::Lz4,
        ]
    }

    fn name(&self) -> &'static str {
        "compression"
    }

    fn compare_bytes(
        &self,
        engine: &DiffEngine,
        args: CompareArgs<'_>,
        ctx: &mut Ctx,
    ) -> Result<DiffNode> {
        if !can_expand(args.depth, engine.options.max_depth) {
            return Ok(depth_limit_node(
                args.label,
                &format!("nested {} depth limit reached", args.kind.label()),
            ));
        }
        let (left_inner, right_inner) = match args.kind {
            FileKind::Gzip => (
                engine.decompress_gzip(args.left, args.label, ctx),
                engine.decompress_gzip(args.right, args.label, ctx),
            ),
            FileKind::Zstd => (
                engine.decompress_zstd(args.left, args.label, ctx),
                engine.decompress_zstd(args.right, args.label, ctx),
            ),
            FileKind::Bzip2 => (
                engine.decompress_bzip2(args.left, args.label, ctx),
                engine.decompress_bzip2(args.right, args.label, ctx),
            ),
            FileKind::Xz => (
                engine.decompress_xz(args.left, args.label, ctx),
                engine.decompress_xz(args.right, args.label, ctx),
            ),
            FileKind::Lz4 => (
                engine.decompress_lz4(args.left, args.label, ctx),
                engine.decompress_lz4(args.right, args.label, ctx),
            ),
            _ => unreachable!(),
        };
        let Some(left_inner) = left_inner else {
            return Ok(engine.compare_binary_bytes(args.label, args.left, args.right, ctx));
        };
        let Some(right_inner) = right_inner else {
            return Ok(engine.compare_binary_bytes(args.label, args.left, args.right, ctx));
        };
        let inner_hint = strip_compression_suffix(args.kind, args.hint);
        let left_kind = detect_bytes(inner_hint, &left_inner);
        let right_kind = detect_bytes(inner_hint, &right_inner);
        engine.compare_bytes_internal(
            args.label,
            inner_hint,
            &left_inner,
            &right_inner,
            left_kind,
            right_kind,
            args.depth + 1,
            ctx,
        )
    }
}

/// Restores the pre-decompression hint so nested detection sees the original
/// file name (`.log.gz` -> `.log`; an `.ipk` payload unpacks a `package.tar`).
fn strip_compression_suffix(kind: FileKind, hint: &str) -> &str {
    if kind == FileKind::Gzip && hint.to_ascii_lowercase().ends_with(".ipk") {
        return "package.tar";
    }
    match kind {
        FileKind::Gzip => hint.strip_suffix(".gz").unwrap_or(hint),
        FileKind::Zstd => hint.strip_suffix(".zst").unwrap_or(hint),
        FileKind::Bzip2 => hint.strip_suffix(".bz2").unwrap_or(hint),
        FileKind::Xz => hint.strip_suffix(".xz").unwrap_or(hint),
        FileKind::Lz4 => hint.strip_suffix(".lz4").unwrap_or(hint),
        _ => hint,
    }
}

/// Nested archives: enforce the depth gate, run the format's bounded reader
/// and fall back to a binary comparison when the archive cannot be read.
struct ArchiveParser;

impl FormatParser for ArchiveParser {
    fn kinds(&self) -> &'static [FileKind] {
        &[
            FileKind::Zip,
            FileKind::SevenZip,
            FileKind::Tar,
            FileKind::Deb,
            FileKind::Rpm,
            FileKind::Iso,
            FileKind::Cpio,
            FileKind::Cab,
            FileKind::Xar,
            FileKind::Ar,
        ]
    }

    fn name(&self) -> &'static str {
        "archive"
    }

    fn compare_bytes(
        &self,
        engine: &DiffEngine,
        args: CompareArgs<'_>,
        ctx: &mut Ctx,
    ) -> Result<DiffNode> {
        if !can_expand(args.depth, engine.options.max_depth) {
            return Ok(depth_limit_node(
                args.label,
                "nested archive depth limit reached",
            ));
        }
        let omitted_before = ctx.omitted;
        let result = match args.kind {
            FileKind::Zip => {
                engine.compare_zip_bytes(args.label, args.left, args.right, args.depth, ctx)
            }
            FileKind::SevenZip => {
                engine.compare_7z_bytes(args.label, args.left, args.right, args.depth, ctx)
            }
            FileKind::Tar => {
                engine.compare_tar_bytes(args.label, args.left, args.right, args.depth, ctx)
            }
            FileKind::Deb => {
                engine.compare_deb_bytes(args.label, args.left, args.right, args.depth, ctx)
            }
            FileKind::Rpm => {
                engine.compare_rpm_bytes(args.label, args.left, args.right, args.depth, ctx)
            }
            FileKind::Iso => {
                engine.compare_iso_bytes(args.label, args.left, args.right, args.depth, ctx)
            }
            FileKind::Cpio => {
                engine.compare_cpio_bytes(args.label, args.left, args.right, args.depth, ctx)
            }
            FileKind::Cab => {
                engine.compare_cab_bytes(args.label, args.left, args.right, args.depth, ctx)
            }
            FileKind::Xar => {
                engine.compare_xar_bytes(args.label, args.left, args.right, args.depth, ctx)
            }
            FileKind::Ar => {
                engine.compare_ar_bytes(args.label, args.left, args.right, args.depth, ctx)
            }
            _ => unreachable!(),
        };
        let node = match result {
            Ok(node) => node,
            Err(err) => {
                ctx.warn(format!(
                    "{}: could not read archive ({err}); falling back to binary",
                    display_label(args.label)
                ));
                engine.compare_binary_bytes(args.label, args.left, args.right, ctx)
            }
        };
        // Readers bump `omitted` when a per-entry limit or the resource budget
        // made them skip an entry; the archive is then only partially compared
        // even if every kept entry is equal.
        Ok(complete_unless(node, ctx.omitted > omitted_before))
    }
}

/// Everything without a semantic parser: byte comparison, plus the legacy
/// Office warning for the one kind that never had a reader.
struct BinaryParser;

impl FormatParser for BinaryParser {
    fn kinds(&self) -> &'static [FileKind] {
        &[
            FileKind::Office,
            FileKind::Audio,
            FileKind::Binary,
            FileKind::Unknown,
            FileKind::Directory,
        ]
    }

    fn name(&self) -> &'static str {
        "binary"
    }

    fn compare_bytes(
        &self,
        engine: &DiffEngine,
        args: CompareArgs<'_>,
        ctx: &mut Ctx,
    ) -> Result<DiffNode> {
        if args.kind == FileKind::Office {
            ctx.warn(format!(
                "{}: legacy Office format diff is not implemented; falling back to binary",
                display_label(args.label)
            ));
        }
        Ok(engine.compare_binary_bytes(args.label, args.left, args.right, ctx))
    }
}

static TEXT_PARSER: TextParser = TextParser;
static STRUCTURED_PARSER: StructuredParser = StructuredParser;
static NOTEBOOK_PARSER: NotebookParser = NotebookParser;
static SQLITE_PARSER: SqliteParser = SqliteParser;
static EXECUTABLE_PARSER: ExecutableParser = ExecutableParser;
static DEX_PARSER: DexParser = DexParser;
static FONT_PARSER: FontParser = FontParser;
static TABLE_PARSER: TableParser = TableParser;
static IMAGE_PARSER: ImageParser = ImageParser;
static COMPRESSION_PARSER: CompressionParser = CompressionParser;
static ARCHIVE_PARSER: ArchiveParser = ArchiveParser;
static DOCUMENT_PARSER: DocumentParser = DocumentParser;
static WORKBOOK_PARSER: WorkbookParser = WorkbookParser;
static BINARY_PARSER: BinaryParser = BinaryParser;

/// The engine's parser registry: one entry per format parser, consulted by
/// `compare_bytes_internal` for every comparison. Parsers are found by their
/// declared `kinds`, so new formats plug in without touching the dispatcher.
fn parser_registry() -> &'static [&'static dyn FormatParser] {
    static REGISTRY: OnceLock<Vec<&'static dyn FormatParser>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        vec![
            &TEXT_PARSER,
            &STRUCTURED_PARSER,
            &NOTEBOOK_PARSER,
            &SQLITE_PARSER,
            &EXECUTABLE_PARSER,
            &DEX_PARSER,
            &FONT_PARSER,
            &TABLE_PARSER,
            &IMAGE_PARSER,
            &COMPRESSION_PARSER,
            &ARCHIVE_PARSER,
            &DOCUMENT_PARSER,
            &WORKBOOK_PARSER,
            &BINARY_PARSER,
        ]
    })
}

fn parser_for(kind: FileKind) -> Option<&'static dyn FormatParser> {
    parser_registry()
        .iter()
        .copied()
        .find(|parser| parser.kinds().contains(&kind))
}

#[derive(Default)]
struct WalkedFiles {
    files: BTreeMap<String, PathBuf>,
    /// Directories at the depth limit whose contents were not scanned.
    unscanned: Vec<String>,
    /// True when the walk stopped at the directory entry limit before the
    /// tree was fully enumerated.
    scan_truncated: bool,
}

#[derive(Default)]
struct Ctx {
    summary: Summary,
    warnings: Vec<String>,
    excludes: Option<Arc<GlobSet>>,
    /// Shared across every worker of one comparison.
    budget: Arc<ResourceBudget>,
    /// Entries, levels or streams that a limit omitted. Readers bump this when
    /// they skip something, so the enclosing container can be marked
    /// incomplete.
    omitted: usize,
}

impl Ctx {
    fn warn(&mut self, message: String) {
        if self.warnings.len() < 50 {
            self.warnings.push(message);
        } else if self.warnings.len() == 50 {
            self.warnings
                .push("further warnings suppressed".to_string());
        }
    }

    /// A worker context: fresh counters, shared exclusion matcher and budget.
    fn child(excludes: Option<Arc<GlobSet>>, budget: Arc<ResourceBudget>) -> Self {
        Self {
            summary: Summary::default(),
            warnings: Vec::new(),
            excludes,
            budget,
            omitted: 0,
        }
    }

    /// Records that a limit omitted an entry, level or stream.
    fn omit(&mut self) {
        self.omitted += 1;
    }

    /// Charges `bytes` of unpacked data against the shared budget. `false`
    /// means the budget is spent: the warning is emitted once per comparison
    /// and the caller must stop materializing more data.
    fn charge_expanded(&mut self, label: &str, bytes: usize) -> bool {
        if self.budget.charge(bytes as u64) {
            return true;
        }
        if self.budget.announce_exhaustion() {
            self.warn(format!(
                "{}: expanded-bytes budget exhausted ({} bytes); remaining unpacked data is skipped",
                display_label(label),
                self.budget.max_expanded_bytes()
            ));
        }
        self.omit();
        false
    }

    fn merge(&mut self, other: Self) {
        self.summary.merge(&other.summary);
        self.omitted += other.omitted;
        for warning in other.warnings {
            self.warn(warning);
        }
    }
}

fn fraction(changed: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        (changed as f64 / total as f64).clamp(0.0, 1.0)
    }
}

fn average_changed_ratio(changed: u64, left_total: u64, right_total: u64) -> f64 {
    if left_total == 0 && right_total == 0 {
        return 0.0;
    }
    if left_total == 0 || right_total == 0 {
        return 1.0;
    }
    let changed = changed as f64;
    ((changed / left_total as f64 + changed / right_total as f64) / 2.0).clamp(0.0, 1.0)
}

fn body_difference_ratio(body: &DiffBody) -> f64 {
    match body {
        DiffBody::Text(diff) => diff.byte_difference_ratio.clamp(0.0, 1.0),
        DiffBody::Binary(diff) => {
            average_changed_ratio(diff.differing_bytes, diff.left_len, diff.right_len)
        }
        DiffBody::Image(diff) => diff.diff_ratio.clamp(0.0, 1.0),
        DiffBody::Table(diff) => fraction(
            diff.rows.len() as u64,
            diff.left_rows.max(diff.right_rows) as u64,
        ),
        DiffBody::Workbook(diff) => {
            let left_cells: u64 = diff
                .sheets
                .iter()
                .map(|sheet| u64::from(sheet.left_rows) * u64::from(sheet.left_columns))
                .sum();
            let right_cells: u64 = diff
                .sheets
                .iter()
                .map(|sheet| u64::from(sheet.right_rows) * u64::from(sheet.right_columns))
                .sum();
            let changed: u64 = diff
                .sheets
                .iter()
                .map(|sheet| sheet.cells.len() as u64)
                .sum();
            fraction(changed, left_cells.max(right_cells))
        }
        DiffBody::Notebook(diff) => fraction(
            (diff.cells_added + diff.cells_removed + diff.changed_cells.len()) as u64,
            diff.left_cells.max(diff.right_cells) as u64,
        ),
        DiffBody::Audio(diff) => diff.waveform.as_ref().map_or(0.0, |waveform| {
            fraction(
                waveform.changed_windows as u64,
                waveform.left.len().max(waveform.right.len()) as u64,
            )
        }),
        DiffBody::OneSided(_) => 1.0,
        _ => 0.0,
    }
}

fn is_completely_different(root: &DiffNode) -> bool {
    if root.status == DiffStatus::Equal {
        return false;
    }
    let ratio = match &root.body {
        DiffBody::Directory(diff) => {
            let changed = diff
                .entries
                .iter()
                .filter(|entry| entry.status.is_different())
                .count() as u64;
            fraction(changed, diff.total_entries as u64)
        }
        DiffBody::Archive(diff) => {
            let changed = diff
                .entries
                .iter()
                .filter(|entry| entry.status.is_different())
                .count() as u64;
            fraction(changed, diff.total_entries as u64)
        }
        body => body_difference_ratio(body),
    };
    ratio > COMPLETELY_DIFFERENT_THRESHOLD
}

struct EntryComparison {
    entries: Vec<DiffNode>,
    total_entries: usize,
    truncated: bool,
}

fn merged_keys<T>(left: &BTreeMap<String, T>, right: &BTreeMap<String, T>) -> Vec<String> {
    let mut merged = Vec::with_capacity(left.len().saturating_add(right.len()));
    let mut left_keys = left.keys().peekable();
    let mut right_keys = right.keys().peekable();
    loop {
        match (left_keys.peek(), right_keys.peek()) {
            (Some(left_key), Some(right_key)) => match left_key.cmp(right_key) {
                std::cmp::Ordering::Less => {
                    merged.push((*left_key).clone());
                    left_keys.next();
                }
                std::cmp::Ordering::Greater => {
                    merged.push((*right_key).clone());
                    right_keys.next();
                }
                std::cmp::Ordering::Equal => {
                    merged.push((*left_key).clone());
                    left_keys.next();
                    right_keys.next();
                }
            },
            (Some(left_key), None) => {
                merged.push((*left_key).clone());
                left_keys.next();
            }
            (None, Some(right_key)) => {
                merged.push((*right_key).clone());
                right_keys.next();
            }
            (None, None) => break,
        }
    }
    merged
}

fn entries_status(entries: &[DiffNode]) -> DiffStatus {
    if entries.iter().any(|entry| entry.status.is_different()) {
        DiffStatus::Different
    } else {
        DiffStatus::Equal
    }
}

/// Depth semantics shared by every recursive path. `depth` is the number of
/// container levels between the input root (0) and the current node; unpacking
/// a compressed stream or an archive entry counts as one level, exactly like a
/// directory.
///
/// Leaves (files, archive entries) are compared while `depth <= limit`.
/// Containers (directories, archives, compressed streams) may only expand
/// while `depth < limit`, so their children always land at `depth + 1 <= limit`
/// and a container at the limit reports a single unsupported node instead of
/// enumerating children that could only be rejected.
fn can_expand(depth: usize, limit: usize) -> bool {
    depth < limit
}

fn depth_limit_node(label: &str, note: &str) -> DiffNode {
    DiffNode::new(
        label,
        DiffStatus::Unsupported,
        DiffBody::Unsupported(UnsupportedInfo {
            note: note.to_string(),
        }),
    )
    .with_completeness(ComparisonCompleteness::Truncated)
}

/// Marks a container node incomplete when a limit truncated it.
fn extract_document_text(kind: FileKind, bytes: &[u8]) -> std::result::Result<String, String> {
    match kind {
        FileKind::Pdf => documents::extract_pdf_text(bytes),
        FileKind::Ofd => documents::extract_ofd_text(bytes),
        FileKind::Docx => documents::extract_docx_text(bytes),
        FileKind::Pptx => documents::extract_pptx_text(bytes),
        FileKind::Odt => crate::diff::documents_extra::extract_odt_text(bytes),
        FileKind::Odp => crate::diff::documents_extra::extract_odp_text(bytes),
        other => Err(format!("no document text extractor for {}", other.label())),
    }
}

fn document_status(diff: &DocumentDiff) -> DiffStatus {
    let table_changed = diff.tables.iter().any(|table| {
        !table.table.rows.is_empty()
            || !table.table.columns_added.is_empty()
            || !table.table.columns_removed.is_empty()
    });
    if diff.metadata.is_empty() && diff.blocks.is_empty() && !table_changed && diff.media.is_empty()
    {
        DiffStatus::Equal
    } else {
        DiffStatus::Different
    }
}

fn budget_error_node(label: &str, error: &str, ctx: &mut Ctx) -> Option<DiffNode> {
    if !error.contains("expanded-bytes budget exhausted") {
        return None;
    }
    ctx.warn(format!(
        "{}: {error}; remaining document parts were not compared",
        display_label(label)
    ));
    ctx.omit();
    Some(
        DiffNode::new(
            label,
            DiffStatus::Unsupported,
            DiffBody::Unsupported(UnsupportedInfo {
                note: error.to_string(),
            }),
        )
        .with_completeness(ComparisonCompleteness::Truncated),
    )
}

fn complete_unless(node: DiffNode, truncated: bool) -> DiffNode {
    if truncated {
        node.with_completeness(ComparisonCompleteness::Truncated)
    } else {
        node
    }
}

/// Reads at most `limit + 1` bytes from `reader` so the caller can tell a
/// within-limit payload from one that exceeds it. `limit` is either the
/// configured per-file limit or `UNLIMITED_FILE_BYTES`, which leaves room for
/// the extra byte.
fn read_bounded<R: std::io::Read>(
    reader: R,
    limit: u64,
    into: &mut Vec<u8>,
) -> std::io::Result<usize> {
    reader.take(limit.saturating_add(1)).read_to_end(into)
}

/// Bounded file read: even if the file grows after its metadata was checked,
/// at most `limit + 1` bytes are materialized (TOCTOU).
fn read_file_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let mut file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    read_bounded(&mut file, limit, &mut bytes)?;
    Ok(bytes)
}

/// Pre-allocation for a size declared inside an untrusted file. The declaration
/// has already been checked against the per-file limit, but under
/// `--no-file-size-limit` that check is vacuous, so cap the reservation as well.
const MAX_DECLARED_RESERVE: u64 = 16 * 1024 * 1024;

fn reserve_for_declared(declared: u64) -> usize {
    declared.min(MAX_DECLARED_RESERVE) as usize
}

fn decode_zip_lzma(
    compressed: &[u8],
    size: u64,
    limit: u64,
) -> std::result::Result<Vec<u8>, String> {
    if compressed.len() < 9 {
        return Err("truncated LZMA header".to_string());
    }
    let properties_size = u16::from_le_bytes([compressed[2], compressed[3]]) as usize;
    if properties_size != 5 || compressed.len() < 4 + properties_size {
        return Err(format!(
            "unsupported LZMA properties size {properties_size}"
        ));
    }
    // ZIP method 14 stores 2 version bytes, 2 property-size bytes, the 5
    // LZMA-Alone properties, and then the raw stream. Rebuild a standard
    // .lzma header so the pure-Rust decoder can read it.
    let mut stream = Vec::with_capacity(13 + compressed.len() - 4 - properties_size);
    stream.extend_from_slice(&compressed[4..9]);
    stream.extend_from_slice(&size.to_le_bytes());
    stream.extend_from_slice(&compressed[9..]);
    let mut output = LimitedBuffer::new(limit);
    lzma_rs::lzma_decompress(&mut std::io::Cursor::new(stream), &mut output)
        .map_err(|error| error.to_string())?;
    Ok(output.data)
}

struct LimitedBuffer {
    data: Vec<u8>,
    limit: u64,
}

impl LimitedBuffer {
    fn new(limit: u64) -> Self {
        Self {
            data: Vec::new(),
            limit,
        }
    }
}

impl Write for LimitedBuffer {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if self.data.len() as u64 + buffer.len() as u64 > self.limit {
            return Err(std::io::Error::other(
                "zip LZMA output exceeds the detailed-diff limit",
            ));
        }
        self.data.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn cpio_field(header: &[u8], start: usize) -> Result<usize> {
    let text = std::str::from_utf8(&header[start..start + 8])
        .map_err(|_| DiffError::Invalid("cpio: invalid header field".into()))?;
    usize::from_str_radix(text, 16)
        .map_err(|_| DiffError::Invalid("cpio: invalid header field".into()))
}

fn hash_file_with_gate(gate: &InflightGate, path: &Path) -> Option<String> {
    let size = fs::metadata(path).ok()?.len();
    let _permit = gate.acquire(size);
    file_sha256(path)
}

fn file_sha256(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Some(hex_bytes(&hasher.finalize()))
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn detect_directory_renames(
    gate: &InflightGate,
    left_only: &[String],
    right_only: &[String],
    left_files: &BTreeMap<String, PathBuf>,
    right_files: &BTreeMap<String, PathBuf>,
) -> BTreeMap<String, String> {
    if left_only.is_empty() || right_only.is_empty() {
        return BTreeMap::new();
    }
    let mut right_by_size: BTreeMap<u64, Vec<&String>> = BTreeMap::new();
    for key in right_only {
        if let Ok(metadata) = fs::metadata(&right_files[key]) {
            right_by_size.entry(metadata.len()).or_default().push(key);
        }
    }
    let mut right_hashes: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut used_right = BTreeSet::new();
    let mut pairs = BTreeMap::new();
    for left_key in left_only {
        let left_path = &left_files[left_key];
        let Ok(size) = fs::metadata(left_path).map(|metadata| metadata.len()) else {
            continue;
        };
        let Some(candidates) = right_by_size.get(&size) else {
            continue;
        };
        let Some(left_hash) = hash_file_with_gate(gate, left_path) else {
            continue;
        };
        for candidate in candidates {
            if used_right.contains(*candidate) {
                continue;
            }
            let hash = right_hashes
                .entry((*candidate).clone())
                .or_insert_with(|| hash_file_with_gate(gate, &right_files[*candidate]));
            if hash.as_deref() == Some(left_hash.as_str()) {
                used_right.insert((*candidate).clone());
                pairs.insert(left_key.clone(), (*candidate).clone());
                break;
            }
        }
    }
    pairs
}

fn detect_archive_renames(
    left_only: &[String],
    right_only: &[String],
    left_entries: &BTreeMap<String, Vec<u8>>,
    right_entries: &BTreeMap<String, Vec<u8>>,
) -> BTreeMap<String, String> {
    if left_only.is_empty() || right_only.is_empty() {
        return BTreeMap::new();
    }
    let mut pairs = BTreeMap::new();
    let mut used_right = BTreeSet::new();
    for left_key in left_only {
        let left_data = &left_entries[left_key];
        let left_hash = hex_bytes(&Sha256::digest(left_data));
        for candidate in right_only {
            if used_right.contains(candidate) {
                continue;
            }
            let right_data = &right_entries[candidate];
            if right_data.len() == left_data.len()
                && hex_bytes(&Sha256::digest(right_data)) == left_hash
            {
                used_right.insert(candidate.clone());
                pairs.insert(left_key.clone(), candidate.clone());
                break;
            }
        }
    }
    pairs
}

fn effective_kind(left: FileKind, right: FileKind) -> FileKind {
    if left == right {
        return left;
    }
    if left.is_structured() && right.is_structured() {
        return FileKind::Structured;
    }
    if left.is_document() && right.is_document() {
        return FileKind::DocumentCross;
    }
    if left.is_textual() && right.is_textual() {
        return FileKind::Text;
    }
    if left == FileKind::Unknown {
        return right;
    }
    if right == FileKind::Unknown {
        return left;
    }
    FileKind::Binary
}

fn equal_node(label: &str, kind: FileKind, note: &str) -> DiffNode {
    DiffNode::new(
        label,
        DiffStatus::Equal,
        DiffBody::Equal(EqualInfo {
            kind,
            note: note.to_string(),
        }),
    )
}

fn display_label(label: &str) -> &str {
    if label.is_empty() { "<root>" } else { label }
}

fn audio_tags(file: &lofty::file::TaggedFile) -> BTreeMap<String, String> {
    use lofty::file::TaggedFileExt;
    let mut tags = BTreeMap::new();
    for tag in file.tags() {
        for item in tag.items() {
            let key = format!("{:?}", item.key());
            let value = item
                .value()
                .text()
                .or_else(|| item.value().locator())
                .unwrap_or("<binary>")
                .to_string();
            tags.insert(key, value);
        }
    }
    tags
}

fn add_executable_byte_changes(
    diff: &mut StructuredDiff,
    left_value: &serde_json::Value,
    right_value: &serde_json::Value,
    left_bytes: &[u8],
    right_bytes: &[u8],
    max_changes: usize,
) {
    let left_sections = executable_sections(left_value);
    let right_sections = executable_sections(right_value);
    for (name, left_section) in left_sections {
        let Some(right_section) = right_sections.get(&name) else {
            continue;
        };
        let left_offset = left_section["offset"].as_u64().unwrap_or(0) as usize;
        let right_offset = right_section["offset"].as_u64().unwrap_or(0) as usize;
        let left_size = left_section["file_size"].as_u64().unwrap_or(0) as usize;
        let right_size = right_section["file_size"].as_u64().unwrap_or(0) as usize;
        if left_size == 0 && right_size == 0 {
            continue;
        }
        let Some(left_data) = left_bytes.get(left_offset..left_offset.saturating_add(left_size))
        else {
            continue;
        };
        let Some(right_data) =
            right_bytes.get(right_offset..right_offset.saturating_add(right_size))
        else {
            continue;
        };
        if left_data == right_data {
            continue;
        }
        let common = left_data.len().min(right_data.len());
        let first = (0..common)
            .find(|index| left_data[*index] != right_data[*index])
            .unwrap_or(common);
        let differing = left_data
            .iter()
            .zip(right_data.iter())
            .filter(|(left, right)| left != right)
            .count()
            + left_data.len().abs_diff(right_data.len());
        let left_absolute = left_offset as u64 + first as u64;
        let right_absolute = right_offset as u64 + first as u64;
        let change = StructuredChange {
            path: format!("$.sections[{name}].byte_diff"),
            kind: StructuredChangeKind::Changed,
            left: Some(serde_json::json!({
                "file_offset": left_offset,
                "absolute_offset": left_absolute,
                "relative_offset": first,
                "differing_bytes": differing,
                "size": left_data.len(),
                "hex": hex_excerpt(left_data, first),
            })),
            right: Some(serde_json::json!({
                "file_offset": right_offset,
                "absolute_offset": right_absolute,
                "relative_offset": first,
                "differing_bytes": differing,
                "size": right_data.len(),
                "hex": hex_excerpt(right_data, first),
            })),
            breaking: false,
        };
        if diff.changes.len() >= max_changes {
            diff.truncated = true;
            break;
        }
        diff.changes.push(change);
    }
}

fn executable_sections(value: &serde_json::Value) -> BTreeMap<String, &serde_json::Value> {
    let mut sections = BTreeMap::new();
    if let Some(items) = value.get("sections").and_then(|value| value.as_array()) {
        for section in items {
            if let Some(name) = section.get("name").and_then(|value| value.as_str()) {
                sections.entry(name.to_string()).or_insert(section);
            }
        }
    }
    sections
}

fn hex_excerpt(data: &[u8], first: usize) -> String {
    let start = first.saturating_sub(8);
    let end = first.saturating_add(24).min(data.len());
    if start >= end {
        return String::new();
    }
    data[start..end]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn structured_format(kind: FileKind, hint: &str) -> String {
    if kind == FileKind::Json {
        let lower = hint.to_ascii_lowercase();
        if lower.ends_with(".ipynb") {
            return "notebook".to_string();
        }
        if lower.ends_with(".json5") {
            return "json5".to_string();
        }
    }
    structured::format_label(kind).to_string()
}

fn spreadsheet_format(hint: &str) -> &'static str {
    let lower = hint.to_ascii_lowercase();
    if lower.ends_with(".ods") {
        "ods"
    } else if lower.ends_with(".xls") {
        "xls"
    } else {
        "xlsx"
    }
}

struct StreamCompare {
    equal: bool,
    differing_bytes: u64,
    first_difference: Option<u64>,
}

fn compare_streams(left: &Path, right: &Path) -> Result<StreamCompare> {
    let mut left_reader = BufReader::new(fs::File::open(left)?);
    let mut right_reader = BufReader::new(fs::File::open(right)?);
    let mut left_buffer = [0u8; 64 * 1024];
    let mut right_buffer = [0u8; 64 * 1024];

    let mut offset = 0u64;
    let mut differing_bytes = 0u64;
    let mut first_difference = None;

    loop {
        let left_len = read_chunk(&mut left_reader, &mut left_buffer)?;
        let right_len = read_chunk(&mut right_reader, &mut right_buffer)?;
        if left_len == 0 && right_len == 0 {
            break;
        }

        let common = left_len.min(right_len);
        for index in 0..common {
            if left_buffer[index] != right_buffer[index] {
                differing_bytes += 1;
                if first_difference.is_none() {
                    first_difference = Some(offset + index as u64);
                }
            }
        }

        if left_len != right_len {
            let extra = left_len.max(right_len) - common;
            differing_bytes += extra as u64;
            if first_difference.is_none() {
                first_difference = Some(offset + common as u64);
            }
        }

        offset += left_len as u64;
    }

    Ok(StreamCompare {
        equal: differing_bytes == 0,
        differing_bytes,
        first_difference,
    })
}

fn read_chunk<R: Read>(reader: &mut R, buffer: &mut [u8]) -> std::io::Result<usize> {
    let mut total = 0;
    while total < buffer.len() {
        match reader.read(&mut buffer[total..])? {
            0 => break,
            read => total += read,
        }
    }
    Ok(total)
}

#[cfg(test)]
#[path = "../tests/unit/engine.rs"]
mod tests;
