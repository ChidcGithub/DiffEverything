#![forbid(unsafe_code)]

use anyhow::{Context, Result};
use clap::builder::styling::{AnsiColor, Styles};
use clap::{Parser, ValueEnum};
use diff_core::engine::{DiffMode, UNLIMITED_FILE_BYTES};
use diff_core::model::{ComparisonCompleteness, TOOL_VERSION};
use diff_core::packers::detect_packers;
use diff_core::progress::ProgressHandle;
use diff_core::render::{self, RenderOptions};
use diff_core::security::{self, ScanOutcome};
use diff_core::{DiffEngine, DiffOptions, DiffReport, DiffStatus, FileKind};
use std::ffi::OsString;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

mod error_report;
mod link;
mod remote;
mod support;
mod tui;

const HELP_STYLES: Styles = Styles::styled()
    .header(AnsiColor::BrightCyan.on_default().bold())
    .usage(AnsiColor::BrightCyan.on_default().bold())
    .literal(AnsiColor::BrightGreen.on_default().bold())
    .placeholder(AnsiColor::BrightBlue.on_default())
    .valid(AnsiColor::BrightGreen.on_default())
    .invalid(AnsiColor::BrightRed.on_default())
    .error(AnsiColor::BrightRed.on_default().bold());

const LONG_VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (Toyger)\n",
    "Commit:     ",
    env!("BUILD_COMMIT"),
    "\n",
    "Build:      ",
    env!("BUILD_PROFILE"),
    " / ",
    env!("BUILD_TARGET"),
    "\n",
    "Rust:       ",
    env!("BUILD_RUSTC"),
    "\n",
    "License:    ",
    env!("CARGO_PKG_LICENSE"),
    "\n",
    "Repository: ",
    env!("CARGO_PKG_REPOSITORY")
);

const CLI_AFTER_HELP: &str = "\
Examples:
  diff-everything old.txt new.txt
  diff-everything left/ right/ --jobs 8 --progress
  diff-everything old.apk new.apk --detail full -y
  diff-everything old.docx new.docx --detail full
  diff-everything old.xlsx new.xlsx --compare-formulas
  diff-everything https://host/old.zip https://host/new.zip
  diff-everything repo-a repo-b --git-history --detail full

Exit codes: 0 same, 1 differences or merge conflicts, 2 runtime error,
3 equal but the comparison was truncated by a limit.
Use --style rich for the decorated terminal layout; --style plain keeps the compact layout.";

#[derive(Debug, Parser)]
#[command(
    name = "diff-everything",
    bin_name = "diff-everything",
    version = TOOL_VERSION,
    long_version = LONG_VERSION,
    about = "Compare files, folders, archives, documents, media, links and Git histories",
    long_about = "DiffEverything compares files, folders, archives, documents, media, binaries, remote links and Git repositories. Formats are detected automatically and reports are rendered as ANSI, TUI, JSON, Markdown, HTML or unified diff.",
    after_help = CLI_AFTER_HELP,
    override_usage = "diff-everything [OPTIONS] [LEFT] [RIGHT]",
    styles = HELP_STYLES,
    max_term_width = 100
)]
struct Cli {
    /// Left / old input (optional with --load-session)
    left: Option<PathBuf>,
    /// Right / new input (optional with --load-session)
    right: Option<PathBuf>,

    /// Lines of context around text changes
    #[arg(
        help_heading = "Comparison",
        long,
        default_value_t = 3,
        value_name = "N"
    )]
    context: usize,

    /// Text diff mode: line, tree-sitter token, syntax tree, or graph events
    #[arg(
        help_heading = "Comparison",
        long,
        value_enum,
        default_value_t = Mode::Text,
        value_name = "MODE"
    )]
    mode: Mode,

    /// Ignore comments (enables token mode for supported source languages)
    #[arg(help_heading = "Comparison", long)]
    ignore_comments: bool,

    /// Ignore whitespace-only changes in text comparisons
    #[arg(help_heading = "Comparison", long)]
    ignore_whitespace: bool,

    /// Ignore case-only changes in text comparisons
    #[arg(help_heading = "Comparison", long)]
    ignore_case: bool,

    /// Column used to align CSV/TSV rows (default: row index)
    #[arg(help_heading = "Comparison", long, value_name = "COLUMN")]
    key: Option<String>,

    /// Comma-separated CSV/TSV column names or spreadsheet column letters to ignore
    #[arg(
        help_heading = "Comparison",
        long,
        value_delimiter = ',',
        value_name = "COLUMNS"
    )]
    ignore_columns: Vec<String>,

    /// Absolute tolerance used when both CSV/TSV cells parse as numbers
    #[arg(
        help_heading = "Comparison",
        long,
        default_value_t = 0.0,
        value_name = "N"
    )]
    numeric_tolerance: f64,

    /// Also compare spreadsheet formulas (by default only cached values are compared)
    #[arg(help_heading = "Comparison", long)]
    compare_formulas: bool,

    /// Exclude paths in directory comparisons; may be repeated
    #[arg(help_heading = "Comparison", long = "exclude", value_name = "GLOB")]
    exclude: Vec<String>,

    /// Compare the history of two Git repositories or repository URLs
    #[arg(help_heading = "Comparison", long)]
    git_history: bool,

    /// Output format
    #[arg(
        help_heading = "Output & display",
        short,
        long,
        value_enum,
        default_value_t = Format::Ansi,
        value_name = "FORMAT"
    )]
    format: Format,

    /// Write the report to a file instead of stdout
    #[arg(help_heading = "Output & display", short, long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// Detail level: concise (default 10), full, or a numeric change limit
    #[arg(
        help_heading = "Output & display",
        long,
        value_parser = parse_detail,
        default_value = "concise",
        value_name = "N|concise|full"
    )]
    detail: DetailMode,

    /// Terminal layout: auto, plain, or rich (Ansi/TUI only)
    #[arg(
        help_heading = "Output & display",
        long,
        value_enum,
        default_value_t = StyleMode::Auto,
        value_name = "STYLE"
    )]
    style: StyleMode,

    /// Only print the summary and changed paths
    #[arg(help_heading = "Output & display", long)]
    summary_only: bool,

    /// Print changed paths only
    #[arg(help_heading = "Output & display", long)]
    name_only: bool,

    /// No output; use the exit code only
    #[arg(help_heading = "Output & display", long)]
    quiet: bool,

    /// Disable ANSI colors
    #[arg(help_heading = "Output & display", long)]
    no_color: bool,

    /// Show only the first N detail units per file/entry
    #[arg(
        help_heading = "Output & display",
        long,
        value_name = "N",
        conflicts_with = "last"
    )]
    head: Option<usize>,

    /// Show only the last N detail units per file/entry
    #[arg(
        help_heading = "Output & display",
        long,
        value_name = "N",
        conflicts_with = "head"
    )]
    last: Option<usize>,

    /// Keep paths/labels or detail content matching PATTERN; may be repeated
    #[arg(
        help_heading = "Output & display",
        long = "filter",
        value_name = "PATTERN"
    )]
    filter: Vec<String>,

    /// Force progress output even when stderr is not a terminal
    #[arg(help_heading = "Progress", long, conflicts_with = "no_progress")]
    progress: bool,

    /// Hide progress output (auto-enabled when stderr is a terminal)
    #[arg(help_heading = "Progress", long)]
    no_progress: bool,

    /// Remove all analysis limits (file size, expanded bytes, in-flight bytes,
    /// entries, rows, cells, depth and PDF pages)
    #[arg(
        help_heading = "Limits",
        long,
        conflicts_with_all = [
            "max_rows",
            "max_table_rows",
            "max_workbook_cells",
            "max_file_bytes",
            "no_file_size_limit",
            "max_entries",
            "no_entry_limit",
            "max_depth",
            "max_expanded_bytes",
            "max_inflight_bytes",
            "pdf_max_pages"
        ]
    )]
    no_limits: bool,

    /// Maximum hex rows / detail rows
    #[arg(help_heading = "Limits", long, default_value_t = 200, value_name = "N")]
    max_rows: usize,

    /// Maximum changed rows in a CSV/TSV table
    #[arg(help_heading = "Limits", long, default_value_t = 500, value_name = "N")]
    max_table_rows: usize,

    /// Maximum changed cells in a workbook sheet
    #[arg(
        help_heading = "Limits",
        long,
        default_value_t = 5000,
        value_name = "N"
    )]
    max_workbook_cells: usize,

    /// Maximum bytes per file for detailed diffing (overrides the 64 MiB default)
    #[arg(help_heading = "Limits", long, value_name = "BYTES")]
    max_file_bytes: Option<u64>,

    /// Remove the per-file detailed-diff size limit (may exhaust memory)
    #[arg(help_heading = "Limits", long, conflicts_with = "max_file_bytes")]
    no_file_size_limit: bool,

    /// Maximum entries in a directory, archive, document or presentation
    #[arg(help_heading = "Limits", long, value_name = "N")]
    max_entries: Option<usize>,

    /// Remove directory, archive, document and presentation entry limits
    #[arg(help_heading = "Limits", long, conflicts_with_all = ["max_entries", "no_limits"])]
    no_entry_limit: bool,

    /// Maximum recursive archive/directory depth
    #[arg(help_heading = "Limits", long, default_value_t = 32, value_name = "N")]
    max_depth: usize,

    /// Total unpacked bytes (archive entries and decompressed streams, every
    /// nesting level, both sides) one comparison may materialize
    #[arg(
        help_heading = "Limits",
        long,
        default_value_t = diff_core::engine::DEFAULT_MAX_EXPANDED_BYTES,
        value_name = "BYTES"
    )]
    max_expanded_bytes: u64,

    /// Maximum bytes of file contents held in flight at the same time by the
    /// parallel workers; 0 disables the bound
    #[arg(
        help_heading = "Limits",
        long,
        default_value_t = diff_core::engine::DEFAULT_MAX_INFLIGHT_BYTES,
        value_name = "BYTES"
    )]
    max_inflight_bytes: u64,

    /// Write a visual heatmap for two image inputs
    #[arg(help_heading = "Images", long, value_name = "FILE")]
    image_heatmap: Option<PathBuf>,

    /// Write a side-by-side image for two image inputs
    #[arg(help_heading = "Images", long, value_name = "FILE")]
    image_side_by_side: Option<PathBuf>,

    /// Image comparison tolerance (0..=1); larger values ignore subtle color changes
    #[arg(
        help_heading = "Images",
        long,
        default_value_t = 0.0,
        value_name = "0..1"
    )]
    image_threshold: f64,

    /// Render PDF pages and compare them visually (requires pdftoppm)
    #[arg(help_heading = "PDF & OCR", long)]
    pdf_visual: bool,

    /// Path to pdftoppm, if not on PATH
    #[arg(help_heading = "PDF & OCR", long, value_name = "FILE")]
    pdftoppm: Option<PathBuf>,

    /// DPI for PDF page rendering
    #[arg(
        help_heading = "PDF & OCR",
        long,
        default_value_t = 100,
        value_name = "N"
    )]
    pdf_dpi: u32,

    /// Maximum PDF pages to render
    #[arg(
        help_heading = "PDF & OCR",
        long,
        default_value_t = 50,
        value_name = "N"
    )]
    pdf_max_pages: usize,

    /// Directory for PDF visual images and report.html
    #[arg(help_heading = "PDF & OCR", long, value_name = "DIR")]
    pdf_output_dir: Option<PathBuf>,

    /// Compare images by OCR text (requires tesseract)
    #[arg(help_heading = "PDF & OCR", long)]
    ocr: bool,

    /// Path to tesseract, if not on PATH
    #[arg(help_heading = "PDF & OCR", long, value_name = "FILE")]
    tesseract: Option<PathBuf>,

    /// Directory containing tessdata/*.traineddata
    #[arg(help_heading = "PDF & OCR", long, value_name = "DIR")]
    tessdata: Option<PathBuf>,

    /// Tesseract language, for example eng or chi_sim
    #[arg(
        help_heading = "PDF & OCR",
        long,
        default_value = "eng",
        value_name = "LANG"
    )]
    ocr_language: String,

    /// Three-way merge base file or directory; enables merge mode
    #[arg(help_heading = "Merge & sync", long, value_name = "FILE|DIR")]
    base: Option<PathBuf>,

    /// Output file (or empty directory for folder merge) for three-way merge
    #[arg(help_heading = "Merge & sync", long, value_name = "FILE|DIR")]
    merge_output: Option<PathBuf>,

    /// Folder sync plan direction
    #[arg(help_heading = "Merge & sync", long, value_enum, value_name = "MODE")]
    sync: Option<SyncModeArg>,

    /// Apply the folder sync plan instead of only printing it
    #[arg(help_heading = "Merge & sync", long)]
    sync_apply: bool,

    /// Backup overwritten/deleted files here when applying sync
    #[arg(help_heading = "Merge & sync", long, value_name = "DIR")]
    backup_dir: Option<PathBuf>,

    /// Write a bsdiff patch from left to right (directories use a folder patch)
    #[arg(help_heading = "Patches", long, value_name = "FILE")]
    patch_output: Option<PathBuf>,

    /// Patch container format for --patch-output/--apply-patch
    #[arg(
        help_heading = "Patches",
        long,
        value_enum,
        default_value_t = PatchFormatArg::Bsdiff,
        value_name = "FORMAT"
    )]
    patch_format: PatchFormatArg,

    /// Compression for --patch-format hdiff-rust (files and directories)
    #[arg(
        help_heading = "Patches",
        long,
        value_enum,
        default_value_t = PatchCompressArg::None,
        value_name = "ALGORITHM"
    )]
    patch_compress: PatchCompressArg,

    /// Path to hdiffz, if not on PATH (used with --patch-format hdiff)
    #[arg(help_heading = "Patches", long, value_name = "FILE")]
    hdiffz: Option<PathBuf>,

    /// Path to hpatchz, if not on PATH (used with --patch-format hdiff)
    #[arg(help_heading = "Patches", long, value_name = "FILE")]
    hpatchz: Option<PathBuf>,

    /// Apply a folder patch or HDiffPatch to LEFT and write to --merge-output
    #[arg(help_heading = "Patches", long, value_name = "FILE")]
    apply_patch: Option<PathBuf>,

    /// Save the comparison paths and options as a session file
    #[arg(help_heading = "Sessions & remote", long, value_name = "FILE")]
    save_session: Option<PathBuf>,

    /// Load comparison paths and options from a session file
    #[arg(help_heading = "Sessions & remote", long, value_name = "FILE")]
    load_session: Option<PathBuf>,

    /// Username for WebDAV/SFTP/SMB inputs
    #[arg(help_heading = "Sessions & remote", long, value_name = "USER")]
    remote_user: Option<String>,

    /// Password for WebDAV/SFTP/SMB inputs
    #[arg(help_heading = "Sessions & remote", long, value_name = "PASSWORD")]
    remote_password: Option<String>,

    /// Verify SFTP host keys against this OpenSSH known_hosts file
    #[arg(help_heading = "Sessions & remote", long, value_name = "FILE")]
    sftp_known_hosts: Option<PathBuf>,

    /// Worker threads for directory/archive comparison (0 = all cores, 1 = sequential)
    #[arg(
        help_heading = "Performance",
        long,
        default_value_t = 4,
        value_name = "N"
    )]
    jobs: usize,

    /// Path to aria2c; defaults to the embedded Windows build, then PATH
    #[arg(help_heading = "Performance", long, value_name = "FILE")]
    aria2: Option<PathBuf>,

    /// Download timeout in seconds for linked files and Git clones
    #[arg(
        help_heading = "Performance",
        long,
        default_value_t = 600,
        value_name = "SECONDS"
    )]
    download_timeout: u64,

    /// Scan both inputs with installed Kaspersky before comparing
    #[arg(help_heading = "Security", long)]
    security_scan: bool,

    /// Disable the automatic Kaspersky scan used by linked-file comparison
    #[arg(help_heading = "Security", long, conflicts_with = "security_scan")]
    no_security_scan: bool,

    /// Path to Kaspersky avp.com; auto-detected when omitted
    #[arg(help_heading = "Security", long, value_name = "FILE")]
    kaspersky: Option<PathBuf>,

    /// Kaspersky scan timeout in seconds
    #[arg(
        help_heading = "Security",
        long,
        default_value_t = 300,
        value_name = "SECONDS"
    )]
    scan_timeout: u64,

    /// Assume yes for prompts (decompilation and downloads over 100 MiB)
    #[arg(help_heading = "Approvals & decompilation", short = 'y', long)]
    yes: bool,

    /// Keep full detail but skip code decompilation (faster binary comparison)
    #[arg(help_heading = "Approvals & decompilation", long)]
    no_decompile: bool,

    /// Optional external objdump path; default uses embedded pure-Rust decoders
    #[arg(help_heading = "Approvals & decompilation", long, value_name = "FILE")]
    objdump: Option<PathBuf>,

    /// Delegate comparison to an external program (LEFT and RIGHT are appended)
    #[arg(help_heading = "External tools", long, value_name = "PROGRAM")]
    external_tool: Option<String>,

    /// Extra argument passed to --external-tool; may be repeated
    #[arg(
        help_heading = "External tools",
        long = "external-arg",
        value_name = "ARG",
        allow_hyphen_values = true
    )]
    external_arg: Vec<String>,

    /// List supported formats, or show details for FORMAT
    #[arg(
        help_heading = "Help & information",
        long,
        value_name = "FORMAT",
        num_args = 0..=1
    )]
    support: Option<Option<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Format {
    Ansi,
    Tui,
    Json,
    Markdown,
    Html,
    Unified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum PatchFormatArg {
    Bsdiff,
    Hdiff,
    HdiffRust,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum PatchCompressArg {
    None,
    Zstd,
    Zlib,
    Lzma2,
    Bzip2,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Mode {
    Text,
    Token,
    Tree,
    Graph,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetailMode {
    Concise(usize),
    Full,
}

fn parse_detail(value: &str) -> std::result::Result<DetailMode, String> {
    match value {
        "full" => Ok(DetailMode::Full),
        "concise" => Ok(DetailMode::Concise(10)),
        other => other
            .parse::<usize>()
            .ok()
            .filter(|limit| *limit > 0)
            .map(DetailMode::Concise)
            .ok_or_else(|| "expected concise, full, or a positive number".to_string()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum StyleMode {
    Auto,
    Plain,
    Rich,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SyncModeArg {
    LeftToRight,
    RightToLeft,
    MirrorLeftToRight,
    MirrorRightToLeft,
}

impl From<SyncModeArg> for diff_core::sync::SyncMode {
    fn from(value: SyncModeArg) -> Self {
        match value {
            SyncModeArg::LeftToRight => Self::LeftToRight,
            SyncModeArg::RightToLeft => Self::RightToLeft,
            SyncModeArg::MirrorLeftToRight => Self::MirrorLeftToRight,
            SyncModeArg::MirrorRightToLeft => Self::MirrorRightToLeft,
        }
    }
}

fn resolve_input(
    spec: &Path,
    temp_dir: &Path,
    auth: &remote::RemoteAuth,
    git_history: bool,
    download: link::DownloadConfig<'_>,
) -> Result<(PathBuf, String)> {
    let text = spec.to_string_lossy().into_owned();
    if git_history {
        let repo = link::resolve_repository(&text, temp_dir, download.progress, download.timeout)?;
        return Ok((repo, text));
    }
    if link::is_http_link(&text) {
        let path = link::resolve_http_input(&text, temp_dir, download)?;
        return Ok((path, text));
    }
    remote::resolve_input(spec, temp_dir, auth)
}

fn main() {
    let code = match run() {
        Ok(code) => code,
        Err(error) => {
            error_report::print(&error);
            2
        }
    };
    std::process::exit(code);
}

fn run() -> Result<i32> {
    let cli = Cli::parse();
    error_report::set_no_color(cli.no_color);
    if let Some(format) = &cli.support {
        if cli.left.is_some() || cli.right.is_some() {
            anyhow::bail!("do not pass LEFT/RIGHT together with --support");
        }
        support::print(format.as_deref())?;
        return Ok(0);
    }
    if !(0.0..=1.0).contains(&cli.image_threshold) {
        anyhow::bail!("--image-threshold must be between 0 and 1");
    }
    if !cli.numeric_tolerance.is_finite() || cli.numeric_tolerance < 0.0 {
        anyhow::bail!("--numeric-tolerance must be a finite non-negative number");
    }
    if cli.head == Some(0) || cli.last == Some(0) {
        anyhow::bail!("--head and --last must be at least 1");
    }
    if !(1..=1_024).contains(&cli.max_depth) {
        anyhow::bail!("--max-depth must be between 1 and 1024");
    }
    if cli.jobs > 1_024 {
        anyhow::bail!("--jobs must be between 0 and 1024");
    }
    if cli.scan_timeout == 0 {
        anyhow::bail!("--scan-timeout must be at least 1 second");
    }
    if cli.download_timeout == 0 {
        anyhow::bail!("--download-timeout must be at least 1 second");
    }
    let content_filter = if cli.head.is_some() || cli.last.is_some() || !cli.filter.is_empty() {
        Some(
            diff_core::filter::ContentFilter::new(cli.head, cli.last, &cli.filter, cli.context)
                .map_err(anyhow::Error::msg)?,
        )
    } else {
        None
    };
    if content_filter.is_some()
        && (cli.pdf_visual || cli.sync.is_some() || cli.base.is_some() || cli.apply_patch.is_some())
    {
        anyhow::bail!("--head/--last/--filter are only supported for diff reports");
    }

    let mut options = DiffOptions {
        context: cli.context,
        max_rows: cli.max_rows,
        max_table_rows: cli.max_table_rows,
        max_workbook_cells: cli.max_workbook_cells,
        key_column: cli.key.clone(),
        ignore_columns: cli.ignore_columns.clone(),
        numeric_tolerance: cli.numeric_tolerance,
        compare_formulas: cli.compare_formulas,
        excludes: cli.exclude.clone(),
        mode: match cli.mode {
            Mode::Text => DiffMode::Text,
            Mode::Token => DiffMode::Token,
            Mode::Tree => DiffMode::Tree,
            Mode::Graph => DiffMode::Graph,
        },
        ignore_comments: cli.ignore_comments,
        ignore_whitespace: cli.ignore_whitespace,
        ignore_case: cli.ignore_case,
        image_threshold: cli.image_threshold,
        jobs: cli.jobs,
        ..DiffOptions::default()
    };
    if let DetailMode::Concise(limit) = cli.detail {
        options.concise_limit = Some(limit);
        options.max_hunks = options.max_hunks.min(limit);
        // Keep one extra change so an exact-limit report is not mislabelled as
        // truncated; concise::apply then trims the display to `limit`.
        options.max_json_changes = options.max_json_changes.min(limit.saturating_add(1));
    }
    if cli.no_limits {
        options.max_file_bytes = UNLIMITED_FILE_BYTES;
        options.max_dir_entries = usize::MAX;
        options.max_zip_entries = usize::MAX;
        options.max_rows = usize::MAX;
        options.max_hunks = usize::MAX;
        options.max_json_changes = usize::MAX;
        options.max_table_rows = usize::MAX;
        options.max_workbook_cells = usize::MAX;
        options.max_doc_blocks = usize::MAX;
        options.max_slides = usize::MAX;
        options.max_depth = usize::MAX;
        options.max_expanded_bytes = UNLIMITED_FILE_BYTES;
        options.max_inflight_bytes = 0;
        if !cli.quiet {
            eprintln!(
                "warning: all analysis limits disabled; very large inputs may exhaust memory or take a long time"
            );
        }
    } else {
        if let Some(max_file_bytes) = cli.max_file_bytes {
            options.max_file_bytes = max_file_bytes;
        } else if cli.no_file_size_limit {
            options.max_file_bytes = UNLIMITED_FILE_BYTES;
            if !cli.quiet {
                eprintln!(
                    "warning: per-file size limit disabled; very large inputs may exhaust memory"
                );
            }
        }
        options.max_depth = cli.max_depth;
        options.max_expanded_bytes = cli.max_expanded_bytes;
        options.max_inflight_bytes = cli.max_inflight_bytes;
        if let Some(max_entries) = cli.max_entries {
            options.max_dir_entries = max_entries;
            options.max_zip_entries = max_entries;
            options.max_doc_blocks = max_entries;
            options.max_slides = max_entries;
        } else if cli.no_entry_limit {
            options.max_dir_entries = usize::MAX;
            options.max_zip_entries = usize::MAX;
            options.max_doc_blocks = usize::MAX;
            options.max_slides = usize::MAX;
            if !cli.quiet {
                eprintln!(
                    "warning: directory/archive/document/slide entry limit disabled; very large inputs may exhaust memory"
                );
            }
        }
    }
    let pdf_max_pages = if cli.no_limits {
        usize::MAX
    } else {
        cli.pdf_max_pages
    };
    if cli.pdf_visual {
        let pdftoppm = resolve_pdftoppm(cli.pdftoppm.as_deref())?;
        options.pdf_visual = Some(diff_core::pdf_visual::PdfVisualOptions {
            pdftoppm,
            dpi: cli.pdf_dpi,
            max_pages: pdf_max_pages,
        });
        options.pdf_visual_output = Some(
            cli.pdf_output_dir
                .clone()
                .unwrap_or_else(|| PathBuf::from("pdf-visual-output")),
        );
    }

    let progress =
        if cli.quiet || cli.no_progress || (!cli.progress && !std::io::stderr().is_terminal()) {
            ProgressHandle::disabled()
        } else {
            ProgressHandle::new(true)
        };

    let (left_spec, right_spec) = if let Some(session_path) = &cli.load_session {
        if cli.left.is_some() || cli.right.is_some() {
            anyhow::bail!("do not pass LEFT/RIGHT together with --load-session");
        }
        let session = diff_core::session::load_session(session_path)
            .with_context(|| format!("failed to load session {}", session_path.display()))?;
        options = session.options;
        (session.left, session.right)
    } else {
        let left = cli
            .left
            .clone()
            .context("LEFT is required unless --load-session is used")?;
        if cli.apply_patch.is_some() {
            if cli.right.is_some() {
                anyhow::bail!("do not pass RIGHT together with --apply-patch");
            }
            (left, PathBuf::new())
        } else {
            let right = cli
                .right
                .clone()
                .context("RIGHT is required unless --load-session is used")?;
            (left, right)
        }
    };

    let left_text = left_spec.to_string_lossy().into_owned();
    let right_text = right_spec.to_string_lossy().into_owned();
    let auto_git_history = link::is_git_url(&left_text) && link::is_git_url(&right_text);
    let git_history = cli.git_history || auto_git_history;
    if !git_history && (link::is_git_url(&left_text) || link::is_git_url(&right_text)) {
        anyhow::bail!(
            "Git repository URLs require --git-history, or pass two .git URLs to enable it automatically"
        );
    }
    if git_history
        && (cli.apply_patch.is_some()
            || cli.sync.is_some()
            || cli.base.is_some()
            || cli.external_tool.is_some()
            || cli.pdf_visual)
    {
        anyhow::bail!(
            "--git-history cannot be combined with patch, sync, merge, external-tool or PDF visual modes"
        );
    }
    let linked_mode = remote::is_remote(&left_spec)
        || remote::is_remote(&right_spec)
        || (link::is_git_url(&left_text) && !left_text.starts_with("file://"))
        || (link::is_git_url(&right_text) && !right_text.starts_with("file://"));
    if cli.sync.is_some() && linked_mode {
        anyhow::bail!(
            "--sync supports local paths only; download or mount remote directories before syncing"
        );
    }

    let remote_temp = tempfile::TempDir::new()
        .context("failed to create temporary directory for remote inputs")?;
    let remote_auth = remote::RemoteAuth {
        user: cli.remote_user.clone(),
        password: cli.remote_password.clone(),
        sftp_known_hosts: cli.sftp_known_hosts.clone(),
    };
    let download = link::DownloadConfig {
        yes: cli.yes,
        aria2: cli.aria2.as_deref(),
        timeout: Duration::from_secs(cli.download_timeout),
        user: remote_auth.user.as_deref(),
        password: remote_auth.password.as_deref(),
        progress: &progress,
    };
    let (left, left_display) = resolve_input(
        &left_spec,
        remote_temp.path(),
        &remote_auth,
        git_history,
        download,
    )?;
    error_report::ensure_exists("left", &left)?;

    if let Some(patch_path) = &cli.apply_patch {
        if cli.patch_output.is_some() || cli.sync.is_some() || cli.base.is_some() {
            anyhow::bail!("--apply-patch cannot be combined with --patch-output, --sync or --base");
        }
        let output_dir = cli
            .merge_output
            .clone()
            .context("--apply-patch requires --merge-output <FILE|DIR>")?;
        if left.is_dir() {
            let patch_bytes = std::fs::read(patch_path)
                .with_context(|| format!("failed to read patch {}", patch_path.display()))?;
            if patch_bytes.starts_with(b"HDIFF19") && cli.patch_format != PatchFormatArg::Hdiff {
                diff_core::patch::apply_pure_dir_hdiff(&left, &patch_bytes, &output_dir)
                    .with_context(|| "failed to apply pure Rust directory patch")?;
                if !cli.quiet {
                    emit(
                        &format!(
                            "hdiff directory patch applied (pure Rust): {}\n",
                            output_dir.display()
                        ),
                        &None,
                    )?;
                }
                return Ok(0);
            }
        }
        if left.is_file() {
            let patch_bytes = std::fs::read(patch_path)
                .with_context(|| format!("failed to read patch {}", patch_path.display()))?;
            if diff_core::patch::is_pure_hdiff(&patch_bytes) {
                let old = std::fs::read(&left)
                    .with_context(|| format!("failed to read {}", left.display()))?;
                let new = diff_core::patch::apply_pure_hdiff(&old, &patch_bytes)
                    .with_context(|| "failed to apply HDIFF13 patch")?;
                std::fs::write(&output_dir, new).with_context(|| {
                    format!("failed to write patched output to {}", output_dir.display())
                })?;
                if !cli.quiet {
                    emit(
                        &format!(
                            "hdiff patch applied (pure Rust): {}\n",
                            output_dir.display()
                        ),
                        &None,
                    )?;
                }
                return Ok(0);
            }
        }
        if cli.patch_format == PatchFormatArg::HdiffRust {
            anyhow::bail!(
                "--patch-format hdiff-rust could not decode this patch with the built-in decoder"
            );
        }
        if cli.patch_format == PatchFormatArg::Hdiff {
            let tool = cli
                .hpatchz
                .clone()
                .unwrap_or_else(|| PathBuf::from("hpatchz"));
            run_tool(
                &tool,
                &[
                    "-f".into(),
                    left.as_os_str().into(),
                    patch_path.as_os_str().into(),
                    output_dir.as_os_str().into(),
                ],
            )?;
            if !cli.quiet {
                emit(
                    &format!("hdiff patch applied: {}\n", output_dir.display()),
                    &None,
                )?;
            }
            return Ok(0);
        }
        let stats = diff_core::folder_patch::apply_folder_patch(
            &left,
            patch_path,
            &output_dir,
            options.max_dir_entries,
        )
        .with_context(|| "failed to apply folder patch")?;
        if !cli.quiet {
            let text = format!(
                "folder patch applied: {} copied, {} added, {} modified, {} deleted\n",
                stats.copied, stats.added, stats.modified, stats.deleted
            );
            emit(&text, &None)?;
        }
        return Ok(0);
    }
    let (right, right_display) = resolve_input(
        &right_spec,
        remote_temp.path(),
        &remote_auth,
        git_history,
        download,
    )?;
    error_report::ensure_exists("right", &right)?;
    let _remote_temp = remote_temp;

    let mut packed_warnings = Vec::new();
    let mut packed = Vec::new();
    for (side, path) in [("left", &left), ("right", &right)] {
        for packer in detect_packers(path) {
            packed.push((side, packer));
        }
    }
    if !packed.is_empty() {
        if !cli.quiet {
            eprintln!("warning: packed/protected application detected:");
            for (side, packer) in &packed {
                eprintln!("  {side}: {} ({})", packer.name, packer.evidence);
            }
            eprintln!(
                "Analyzing or unpacking packed software may violate its license or applicable law."
            );
        }
        if !cli.yes {
            if !io::stdin().is_terminal() {
                anyhow::bail!("packed application detected; rerun with -y to continue");
            }
            eprint!("Continue analysis? [y/N]: ");
            io::stderr().flush().ok();
            let mut answer = String::new();
            io::stdin().read_line(&mut answer)?;
            if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
                eprintln!("analysis cancelled");
                return Ok(2);
            }
        }
        for (side, packer) in packed {
            packed_warnings.push(format!(
                "packed/protected application detected on {side}: {} ({})",
                packer.name, packer.evidence
            ));
        }
        packed_warnings.push(
            "analysis of packed software may have legal or licensing implications".to_string(),
        );
    }

    let auto_scan = linked_mode && !cli.security_scan && !cli.no_security_scan;
    if auto_scan && !cli.quiet {
        eprintln!("Kaspersky: scanning linked inputs by default (use --no-security-scan to skip)");
    }
    if cli.security_scan || auto_scan {
        let outcome = security::scan_paths(
            cli.kaspersky.as_deref(),
            &[left.as_path(), right.as_path()],
            Duration::from_secs(cli.scan_timeout),
            Some(&progress),
        );
        progress.finish();
        match outcome {
            ScanOutcome::Clean => {
                if !cli.quiet {
                    eprintln!("Kaspersky: no threats detected");
                }
            }
            ScanOutcome::Unavailable(message) => {
                if !cli.quiet {
                    eprintln!("warning: {message}");
                }
            }
            ScanOutcome::Failed(message) => {
                anyhow::bail!("Kaspersky scan failed: {message}");
            }
            ScanOutcome::Threats(threats) => {
                eprintln!("Kaspersky detected {} threat(s):", threats.len());
                for threat in &threats {
                    eprintln!("  {threat}");
                }
                return Ok(2);
            }
        }
    }

    if let Some(tool) = &cli.external_tool {
        if cli.patch_output.is_some()
            || cli.apply_patch.is_some()
            || cli.sync.is_some()
            || cli.base.is_some()
        {
            anyhow::bail!("--external-tool cannot be combined with patch, sync or merge modes");
        }
        let output = std::process::Command::new(tool)
            .args(&cli.external_arg)
            .arg(&left)
            .arg(&right)
            .output()
            .with_context(|| format!("failed to run external tool {tool}"))?;
        if !output.stderr.is_empty() {
            io::stderr().write_all(&output.stderr)?;
        }
        let code = output.status.code().unwrap_or(2);
        if code > 1 {
            anyhow::bail!("external tool {tool} exited with status {code}");
        }
        if !cli.quiet {
            if let Some(path) = &cli.output {
                std::fs::write(path, &output.stdout).with_context(|| {
                    format!("failed to write external tool output to {}", path.display())
                })?;
            } else {
                io::stdout().write_all(&output.stdout)?;
                io::stdout().flush()?;
            }
        }
        return Ok(code);
    }

    if let Some(decompile) = resolve_decompile_options(&cli, &left, &right)? {
        options.decompile = Some(decompile);
    }

    if cli.ocr || cli.tesseract.is_some() || cli.tessdata.is_some() {
        let (tesseract, tessdata) =
            resolve_tesseract(cli.tesseract.as_deref(), cli.tessdata.as_deref())?;
        let pdftoppm = match &cli.pdftoppm {
            Some(path) if path.is_file() => Some(path.clone()),
            Some(path) => anyhow::bail!("pdftoppm not found at {}", path.display()),
            None => resolve_pdftoppm(None).ok(),
        };
        options.ocr = Some(diff_core::ocr::OcrOptions {
            tesseract,
            tessdata,
            language: cli.ocr_language.clone(),
            pdftoppm,
            dpi: cli.pdf_dpi,
            max_pages: pdf_max_pages,
        });
    }

    if let Some(session_path) = &cli.save_session {
        let session = diff_core::session::Session::new(
            left_spec.clone(),
            right_spec.clone(),
            options.clone(),
        );
        diff_core::session::save_session(session_path, &session)
            .with_context(|| format!("failed to save session to {}", session_path.display()))?;
    }

    if cli.sync_apply && cli.sync.is_none() {
        anyhow::bail!("--sync-apply requires --sync");
    }

    if let Some(sync_mode) = cli.sync {
        let plan = diff_core::sync::plan_sync(
            &left,
            &right,
            sync_mode.into(),
            &options.excludes,
            options.max_dir_entries,
        )
        .with_context(|| "failed to build folder sync plan")?;

        if cli.sync_apply {
            let stats = diff_core::sync::apply_sync(&plan, cli.backup_dir.as_deref())
                .with_context(|| "failed to apply folder sync")?;
            if !cli.quiet {
                let text = format!(
                    "sync applied: {} copied, {} deleted, {} backed up\n",
                    stats.copied, stats.deleted, stats.backed_up
                );
                emit(&text, &cli.output)?;
            }
            return Ok(0);
        }

        if !cli.quiet {
            let text = if cli.format == Format::Json {
                serde_json::to_string_pretty(&plan)?
            } else {
                render_sync_plan(&plan)
            };
            emit(&text, &cli.output)?;
        }
        return Ok(if plan.is_empty() { 0 } else { 1 });
    }

    if let Some(patch_path) = &cli.patch_output {
        if cli.patch_format == PatchFormatArg::HdiffRust {
            if left.is_dir() && right.is_dir() {
                let patch = match cli.patch_compress {
                    PatchCompressArg::None => {
                        diff_core::patch::create_pure_dir_hdiff(&left, &right)?
                    }
                    PatchCompressArg::Zstd => {
                        diff_core::patch::create_pure_dir_hdiff_zstd(&left, &right)?
                    }
                    PatchCompressArg::Zlib => {
                        diff_core::patch::create_pure_dir_hdiff_zlib(&left, &right)?
                    }
                    PatchCompressArg::Lzma2 => {
                        diff_core::patch::create_pure_dir_hdiff_lzma2(&left, &right)?
                    }
                    PatchCompressArg::Bzip2 => {
                        diff_core::patch::create_pure_dir_hdiff_bzip2(&left, &right)?
                    }
                };
                std::fs::write(patch_path, patch).with_context(|| {
                    format!("failed to write patch to {}", patch_path.display())
                })?;
                if !cli.quiet {
                    emit(
                        &format!(
                            "hdiff-rust directory patch created: {}\n",
                            patch_path.display()
                        ),
                        &None,
                    )?;
                }
                return Ok(0);
            }
            if !left.is_file() || !right.is_file() {
                anyhow::bail!("--patch-format hdiff-rust supports two files or two directories");
            }
            let old = std::fs::read(&left)
                .with_context(|| format!("failed to read {}", left.display()))?;
            let new = std::fs::read(&right)
                .with_context(|| format!("failed to read {}", right.display()))?;
            let patch = match cli.patch_compress {
                PatchCompressArg::None => diff_core::patch::create_pure_hdiff(&old, &new),
                PatchCompressArg::Zstd => diff_core::patch::create_pure_hdiff_zstd(&old, &new),
                PatchCompressArg::Zlib => diff_core::patch::create_pure_hdiff_zlib(&old, &new),
                PatchCompressArg::Lzma2 => diff_core::patch::create_pure_hdiff_lzma2(&old, &new),
                PatchCompressArg::Bzip2 => diff_core::patch::create_pure_hdiff_bzip2(&old, &new),
            };
            std::fs::write(patch_path, patch)
                .with_context(|| format!("failed to write patch to {}", patch_path.display()))?;
            if !cli.quiet {
                emit(
                    &format!("hdiff-rust patch created: {}\n", patch_path.display()),
                    &None,
                )?;
            }
            return Ok(0);
        }
        if cli.patch_format == PatchFormatArg::Hdiff {
            let tool = cli
                .hdiffz
                .clone()
                .unwrap_or_else(|| PathBuf::from("hdiffz"));
            run_tool(
                &tool,
                &[
                    "-f".into(),
                    left.as_os_str().into(),
                    right.as_os_str().into(),
                    patch_path.as_os_str().into(),
                ],
            )?;
            if !cli.quiet {
                emit(
                    &format!("hdiff patch created: {}\n", patch_path.display()),
                    &None,
                )?;
            }
            return Ok(0);
        }
        if left.is_dir() && right.is_dir() {
            let stats = diff_core::folder_patch::create_folder_patch(
                &left,
                &right,
                patch_path,
                &options.excludes,
                options.max_dir_entries,
            )
            .with_context(|| "failed to create folder patch")?;
            if !cli.quiet {
                let text = format!(
                    "folder patch created: {} added, {} modified, {} deleted, {} unchanged\n",
                    stats.added, stats.modified, stats.deleted, stats.unchanged
                );
                emit(&text, &None)?;
            }
            return Ok(0);
        }
        if left.is_dir() || right.is_dir() {
            anyhow::bail!("--patch-output requires two files or two directories");
        }
        let old =
            std::fs::read(&left).with_context(|| format!("failed to read {}", left.display()))?;
        let new =
            std::fs::read(&right).with_context(|| format!("failed to read {}", right.display()))?;
        let patch = diff_core::patch::create_patch(&old, &new)
            .with_context(|| "failed to create binary patch")?;
        std::fs::write(patch_path, patch)
            .with_context(|| format!("failed to write patch to {}", patch_path.display()))?;
        return Ok(0);
    }

    if let Some(base) = &cli.base {
        if base.is_dir() {
            if !left.is_dir() || !right.is_dir() {
                anyhow::bail!("folder merge requires LEFT and RIGHT to be directories");
            }
            let Some(output_dir) = cli.merge_output.clone() else {
                anyhow::bail!("folder merge requires --merge-output <DIR>");
            };
            let result = diff_core::folder_merge::merge_folders(
                &left,
                &right,
                base,
                &output_dir,
                &options.excludes,
                options.max_dir_entries,
            )
            .with_context(|| "three-way folder merge failed")?;
            if !cli.quiet {
                let mut text = format!(
                    "folder merge: {} written, {} deleted, {} conflict(s)\n",
                    result.written,
                    result.deleted,
                    result.conflicts.len()
                );
                for conflict in &result.conflicts {
                    text.push_str(&format!("  conflict: {conflict}\n"));
                }
                emit(&text, &None)?;
            }
            return Ok(if result.clean { 0 } else { 1 });
        }
        let result = diff_core::merge::merge_files(base, &left, &right)
            .with_context(|| "three-way merge failed")?;
        if !cli.quiet {
            let destination = cli.merge_output.clone().or_else(|| cli.output.clone());
            emit(&result.text, &destination)?;
        }
        return Ok(if result.clean { 0 } else { 1 });
    }

    if cli.merge_output.is_some() {
        anyhow::bail!("--merge-output requires --base");
    }

    progress.finish();
    progress.reset();
    let mut report = if git_history {
        link::history_report(
            &left,
            &right,
            &left_display,
            &right_display,
            options.max_json_changes,
            &progress,
        )
        .context("failed to compare Git histories")?
    } else {
        let engine = DiffEngine::new(options).with_progress(progress.clone());
        let mut report = engine.compare(&left, &right).with_context(|| {
            format!(
                "failed to compare {} and {}",
                left.display(),
                right.display()
            )
        })?;
        report.left.path = left_display;
        report.right.path = right_display;
        report
    };
    report.warnings.extend(packed_warnings);

    if (cli.image_heatmap.is_some() || cli.image_side_by_side.is_some())
        && (report.left.kind != FileKind::Image || report.right.kind != FileKind::Image)
    {
        anyhow::bail!("image output options require two image inputs");
    }
    if let Some(path) = &cli.image_heatmap {
        diff_core::image_export::write_heatmap(&left, &right, path)
            .with_context(|| format!("failed to write image heatmap to {}", path.display()))?;
    }
    if let Some(path) = &cli.image_side_by_side {
        diff_core::image_export::write_side_by_side(&left, &right, path)
            .with_context(|| format!("failed to write side-by-side image to {}", path.display()))?;
    }

    if let Some(filter) = &content_filter {
        let matched = diff_core::filter::apply(&mut report, filter);
        if !matched && !cli.filter.is_empty() {
            report
                .warnings
                .push("content filter matched no entries".to_string());
        }
        report
            .warnings
            .push("content filter applied; summary reflects the full comparison".to_string());
    }
    if let DetailMode::Concise(limit) = cli.detail {
        diff_core::concise::apply(&mut report, limit);
    }

    if cli.quiet {
        return Ok(exit_code(&report));
    }

    if cli.name_only {
        let text = changed_paths_text(&report);
        emit(&text, &cli.output)?;
        return Ok(exit_code(&report));
    }

    let rich = match cli.style {
        StyleMode::Auto => io::stdout().is_terminal(),
        StyleMode::Plain => false,
        StyleMode::Rich => true,
    };
    let render_options = RenderOptions {
        color: !cli.no_color
            && cli.output.is_none()
            && std::env::var_os("NO_COLOR").is_none()
            && io::stdout().is_terminal(),
        summary_only: cli.summary_only,
        rich,
    };

    if cli.format == Format::Tui && cli.output.is_none() && io::stdout().is_terminal() {
        let ansi = render::ansi::render(
            &report,
            &RenderOptions {
                color: true,
                summary_only: cli.summary_only,
                rich: true,
            },
        );
        tui::run("DiffEverything", &ansi)?;
        return Ok(exit_code(&report));
    }

    let rendered = match cli.format {
        Format::Ansi | Format::Tui => render::ansi::render(&report, &render_options),
        Format::Json => render::json::render(&report),
        Format::Markdown => render::markdown::render(&report),
        Format::Html => render::html::render(&report),
        Format::Unified => render::unified::render(&report),
    };
    emit(&rendered, &cli.output)?;
    Ok(exit_code(&report))
}

fn exit_code(report: &DiffReport) -> i32 {
    if report.status == DiffStatus::Equal {
        // An equal verdict is only conclusive when everything was compared;
        // a truncated comparison reports "indeterminate" so scripts and CI
        // cannot mistake "equal among the compared part" for "identical".
        if report.completeness == ComparisonCompleteness::Truncated {
            3
        } else {
            0
        }
    } else {
        1
    }
}

fn render_sync_plan(plan: &diff_core::sync::SyncPlan) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "sync {} -> {} ({})\n",
        clean_path(&plan.left),
        clean_path(&plan.right),
        plan.mode.label()
    ));
    for action in &plan.actions {
        match action {
            diff_core::sync::SyncAction::Copy { relative, .. } => {
                out.push_str(&format!("COPY   {relative}\n"));
            }
            diff_core::sync::SyncAction::Delete { relative, .. } => {
                out.push_str(&format!("DELETE {relative}\n"));
            }
        }
    }
    out.push_str(&format!("actions: {}\n", plan.actions.len()));
    out
}

fn resolve_pdftoppm(explicit: Option<&std::path::Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        if path.is_file() {
            return Ok(path.to_path_buf());
        }
        anyhow::bail!("pdftoppm not found at {}", path.display());
    }
    if let Some(path) = std::env::var_os("PDFTOPPM_PATH") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
    }
    if let Some(path) = find_in_path("pdftoppm") {
        return Ok(path);
    }
    for root in [
        PathBuf::from("testdata/external/poppler"),
        PathBuf::from("testdata/poppler"),
    ] {
        if !root.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&root)
            .max_depth(6)
            .into_iter()
            .flatten()
        {
            if entry.file_type().is_file() {
                let name = entry.file_name().to_string_lossy();
                if name == "pdftoppm" || name == "pdftoppm.exe" {
                    return Ok(entry.path().to_path_buf());
                }
            }
        }
    }
    anyhow::bail!(
        "pdftoppm was not found. Install poppler, put pdftoppm on PATH, set PDFTOPPM_PATH, or pass --pdftoppm"
    );
}

fn resolve_tesseract(
    explicit: Option<&std::path::Path>,
    tessdata: Option<&std::path::Path>,
) -> Result<(PathBuf, Option<PathBuf>)> {
    let tesseract = if let Some(path) = explicit {
        if !path.is_file() {
            anyhow::bail!("tesseract not found at {}", path.display());
        }
        path.to_path_buf()
    } else if let Some(path) = std::env::var_os("TESSERACT_PATH") {
        let path = PathBuf::from(path);
        if !path.is_file() {
            anyhow::bail!("TESSERACT_PATH is not a file: {}", path.display());
        }
        path
    } else if let Some(path) = find_in_path("tesseract") {
        path
    } else {
        find_local_tool("tesseract").ok_or_else(|| {
            anyhow::anyhow!(
                "tesseract was not found. Install Tesseract, put it on PATH, set TESSERACT_PATH, or pass --tesseract"
            )
        })?
    };

    let tessdata = if let Some(path) = tessdata {
        if !path.is_dir() {
            anyhow::bail!("tessdata directory not found: {}", path.display());
        }
        Some(path.to_path_buf())
    } else {
        find_tessdata(&tesseract)
    };
    Ok((tesseract, tessdata))
}

fn find_tessdata(tesseract: &std::path::Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(parent) = tesseract.parent() {
        candidates.push(parent.join("tessdata"));
        candidates.push(parent.join("../share/tessdata"));
        candidates.push(parent.join("../tessdata"));
    }
    for root in [
        PathBuf::from("testdata/external/tesseract"),
        PathBuf::from("testdata/tesseract"),
    ] {
        if root.exists() {
            candidates.push(root.join("tessdata"));
        }
    }
    candidates
        .into_iter()
        .find(|candidate| candidate.join("eng.traineddata").is_file())
}

fn find_local_tool(name: &str) -> Option<PathBuf> {
    for root in [
        PathBuf::from("testdata/external/tesseract"),
        PathBuf::from("testdata/tesseract"),
    ] {
        if !root.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&root)
            .max_depth(6)
            .into_iter()
            .flatten()
        {
            if entry.file_type().is_file() {
                let file_name = entry.file_name().to_string_lossy();
                if file_name == name || file_name == format!("{name}.exe") {
                    return Some(entry.path().to_path_buf());
                }
            }
        }
    }
    None
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path) {
        for candidate in [directory.join(name), directory.join(format!("{name}.exe"))] {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn clean_path(path: &std::path::Path) -> String {
    path.to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_string()
}

fn changed_paths_text(report: &diff_core::DiffReport) -> String {
    let entries = render::changed_entries(&report.root);
    if entries.is_empty() {
        return "no differences\n".to_string();
    }
    let mut out = String::new();
    for (marker, path) in entries {
        if path == "<root>" {
            out.push_str(&format!(
                "{marker} {} -> {}\n",
                render::sanitize_controls(&report.left.path),
                render::sanitize_controls(&report.right.path)
            ));
        } else {
            out.push_str(&format!("{marker} {path}\n"));
        }
    }
    out
}

fn resolve_decompile_options(
    cli: &Cli,
    left: &Path,
    right: &Path,
) -> Result<Option<diff_core::engine::DecompileOptions>> {
    if cli.no_decompile
        || cli.detail != DetailMode::Full
        || cli.pdf_visual
        || cli.sync.is_some()
        || cli.base.is_some()
        || cli.apply_patch.is_some()
        || !left.is_file()
        || !right.is_file()
    {
        return Ok(None);
    }

    let left_presence = code_presence(diff_core::detect::detect_file(left), left);
    let right_presence = code_presence(diff_core::detect::detect_file(right), right);
    if !left_presence.has_code() && !right_presence.has_code() {
        return Ok(None);
    }

    let tool = match &cli.objdump {
        Some(path) if path.is_file() => Some(path.clone()),
        Some(path) => anyhow::bail!("objdump not found at {}", path.display()),
        None => None,
    };
    let builtin_dex = tool.is_none() && (left_presence.dex || right_presence.dex);
    // Interactive sessions always get the approval prompt. Headless
    // DEX/APK runs may use the embedded decompiler directly, because
    // --detail full already expresses the request and there is no terminal
    // available to answer a prompt. External objdump always prompts.
    let allow = (builtin_dex && !io::stdin().is_terminal())
        || confirm_decompile(left, right, tool.as_deref(), cli.yes)?;
    Ok(allow.then_some(diff_core::engine::DecompileOptions {
        objdump: tool,
        max_lines: 20_000,
    }))
}

fn code_presence(kind: FileKind, path: &Path) -> diff_core::disassembly::ArchiveCodePresence {
    match kind {
        FileKind::Dex => diff_core::disassembly::ArchiveCodePresence {
            dex: true,
            native: false,
        },
        FileKind::Executable => diff_core::disassembly::ArchiveCodePresence {
            dex: false,
            native: true,
        },
        FileKind::Zip => diff_core::disassembly::archive_code_presence(path),
        _ => diff_core::disassembly::ArchiveCodePresence::default(),
    }
}

fn confirm_decompile(left: &Path, right: &Path, tool: Option<&Path>, yes: bool) -> Result<bool> {
    if yes {
        return Ok(true);
    }
    if !io::stdin().is_terminal() {
        eprintln!(
            "warning: decompilation skipped because stdin is not a terminal; use -y to allow it"
        );
        return Ok(false);
    }
    let tool = tool
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "embedded native/DEX decompiler".to_string());
    eprintln!("warning: --detail full will decompile executable/library/DEX code using {tool}.");
    eprintln!("         left : {}", left.display());
    eprintln!("         right: {}", right.display());
    eprint!("Allow decompiled code comparison? [y/N]: ");
    io::stderr().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

fn run_tool(tool: &Path, args: &[OsString]) -> Result<()> {
    let output = std::process::Command::new(tool)
        .args(args)
        .output()
        .with_context(|| format!("failed to run {}", tool.display()))?;
    if output.status.success() {
        return Ok(());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    anyhow::bail!(
        "{} failed (status {:?}): {}{}",
        tool.display(),
        output.status.code(),
        stderr.trim(),
        stdout.trim()
    )
}

fn emit(text: &str, output: &Option<PathBuf>) -> Result<()> {
    match output {
        Some(path) => {
            std::fs::write(path, text)
                .with_context(|| format!("failed to write report to {}", path.display()))?;
        }
        None => {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            handle.write_all(text.as_bytes())?;
            if !text.ends_with('\n') {
                handle.write_all(b"\n")?;
            }
            handle.flush()?;
        }
    }
    Ok(())
}
