use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use diff_core::engine::DiffMode;
use diff_core::render::{self, RenderOptions};
use diff_core::{DiffEngine, DiffOptions, DiffStatus, FileKind};
use std::ffi::OsString;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

mod remote;
mod tui;

#[derive(Debug, Parser)]
#[command(
    name = "diff-everything",
    version = env!("DIFF_VERSION"),
    about = "Compare text, code, structured data, tables, images, documents, binaries, folders and archives"
)]
struct Cli {
    /// Left / old input (optional with --load-session)
    left: Option<PathBuf>,
    /// Right / new input (optional with --load-session)
    right: Option<PathBuf>,

    /// Output format
    #[arg(help_heading = "Output", short, long, value_enum, default_value_t = Format::Ansi)]
    format: Format,

    /// Write the report to a file instead of stdout
    #[arg(help_heading = "Output", short, long)]
    output: Option<PathBuf>,

    /// Lines of context around text changes
    #[arg(help_heading = "Comparison", long, default_value_t = 3)]
    context: usize,

    /// Maximum hex rows / detail rows
    #[arg(help_heading = "Limits", long, default_value_t = 200)]
    max_rows: usize,

    /// Maximum changed rows in a CSV/TSV table
    #[arg(help_heading = "Limits", long, default_value_t = 500)]
    max_table_rows: usize,

    /// Maximum changed cells in a workbook sheet
    #[arg(help_heading = "Limits", long, default_value_t = 5000)]
    max_workbook_cells: usize,

    /// Column used to align CSV/TSV rows (default: row index)
    #[arg(help_heading = "Comparison", long, value_name = "COLUMN")]
    key: Option<String>,

    /// Exclude paths in directory comparisons; may be repeated
    #[arg(help_heading = "Comparison", long = "exclude", value_name = "GLOB")]
    exclude: Vec<String>,

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

    /// Three-way merge base file or directory; enables merge mode
    #[arg(help_heading = "Merge & sync", long, value_name = "FILE|DIR")]
    base: Option<PathBuf>,

    /// Output file (or empty directory for folder merge) for three-way merge
    #[arg(help_heading = "Merge & sync", long, value_name = "FILE|DIR")]
    merge_output: Option<PathBuf>,

    /// Write a bsdiff patch from left to right (directories use a folder patch)
    #[arg(help_heading = "Patches", long, value_name = "FILE")]
    patch_output: Option<PathBuf>,

    /// Patch container format for --patch-output/--apply-patch
    #[arg(help_heading = "Patches", long, value_enum, default_value_t = PatchFormatArg::Bsdiff)]
    patch_format: PatchFormatArg,

    /// Compression for --patch-format hdiff-rust (files and directories)
    #[arg(help_heading = "Patches", long, value_enum, default_value_t = PatchCompressArg::None)]
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

    /// Folder sync plan direction
    #[arg(help_heading = "Merge & sync", long, value_enum, value_name = "MODE")]
    sync: Option<SyncModeArg>,

    /// Apply the folder sync plan instead of only printing it
    #[arg(help_heading = "Merge & sync", long)]
    sync_apply: bool,

    /// Backup overwritten/deleted files here when applying sync
    #[arg(help_heading = "Merge & sync", long, value_name = "DIR")]
    backup_dir: Option<PathBuf>,

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

    /// Render PDF pages and compare them visually (requires pdftoppm)
    #[arg(help_heading = "PDF & OCR", long)]
    pdf_visual: bool,

    /// Path to pdftoppm, if not on PATH
    #[arg(help_heading = "PDF & OCR", long, value_name = "FILE")]
    pdftoppm: Option<PathBuf>,

    /// DPI for PDF page rendering
    #[arg(help_heading = "PDF & OCR", long, default_value_t = 100)]
    pdf_dpi: u32,

    /// Maximum PDF pages to render
    #[arg(help_heading = "PDF & OCR", long, default_value_t = 50)]
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
    #[arg(help_heading = "PDF & OCR", long, default_value = "eng")]
    ocr_language: String,

    /// Print changed paths only
    #[arg(help_heading = "Output", long)]
    name_only: bool,

    /// No output; use the exit code only
    #[arg(help_heading = "Output", long)]
    quiet: bool,

    /// Disable ANSI colors
    #[arg(help_heading = "Output", long)]
    no_color: bool,

    /// Only print the summary and changed paths
    #[arg(help_heading = "Output", long)]
    summary_only: bool,

    /// Show only the first N detail units per file/entry
    #[arg(
        help_heading = "Output",
        long,
        value_name = "N",
        conflicts_with = "last"
    )]
    head: Option<usize>,

    /// Show only the last N detail units per file/entry
    #[arg(
        help_heading = "Output",
        long,
        value_name = "N",
        conflicts_with = "head"
    )]
    last: Option<usize>,

    /// Keep paths/labels or detail content matching PATTERN; may be repeated
    #[arg(help_heading = "Output", long = "filter", value_name = "PATTERN")]
    filter: Vec<String>,

    /// Detail level: concise or full
    #[arg(help_heading = "Output", long, value_enum, default_value_t = DetailMode::Concise)]
    detail: DetailMode,

    /// Automatically approve decompilation in full detail mode
    #[arg(help_heading = "Decompilation", short = 'y', long)]
    yes: bool,

    /// Optional external objdump path; default uses embedded pure-Rust decoders
    #[arg(help_heading = "Decompilation", long, value_name = "FILE")]
    objdump: Option<PathBuf>,

    /// Text diff mode: line, tree-sitter token, syntax tree, or graph events
    #[arg(help_heading = "Comparison", long, value_enum, default_value_t = Mode::Text)]
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

    /// Maximum bytes per file for detailed diffing
    #[arg(help_heading = "Limits", long)]
    max_file_bytes: Option<u64>,

    /// Maximum entries in a directory or archive
    #[arg(help_heading = "Limits", long)]
    max_entries: Option<usize>,

    /// Maximum recursive archive/directory depth
    #[arg(long, default_value_t = 32, value_name = "N", help_heading = "Limits")]
    max_depth: usize,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum DetailMode {
    Concise,
    Full,
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

fn main() {
    let code = match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error:#}");
            2
        }
    };
    std::process::exit(code);
}

fn run() -> Result<i32> {
    let cli = Cli::parse();
    if !(0.0..=1.0).contains(&cli.image_threshold) {
        anyhow::bail!("--image-threshold must be between 0 and 1");
    }
    if cli.head == Some(0) || cli.last == Some(0) {
        anyhow::bail!("--head and --last must be at least 1");
    }
    if !(1..=1_024).contains(&cli.max_depth) {
        anyhow::bail!("--max-depth must be between 1 and 1024");
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
        ..DiffOptions::default()
    };
    if let Some(max_file_bytes) = cli.max_file_bytes {
        options.max_file_bytes = max_file_bytes;
    }
    options.max_depth = cli.max_depth;
    if let Some(max_entries) = cli.max_entries {
        options.max_dir_entries = max_entries;
        options.max_zip_entries = max_entries;
    }

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

    if cli.sync.is_some() && (remote::is_remote(&left_spec) || remote::is_remote(&right_spec)) {
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
    let (left, left_display) = remote::resolve_input(&left_spec, remote_temp.path(), &remote_auth)?;

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
    let (right, right_display) =
        remote::resolve_input(&right_spec, remote_temp.path(), &remote_auth)?;
    let _remote_temp = remote_temp;

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

    if cli.detail == DetailMode::Full
        && !cli.pdf_visual
        && cli.sync.is_none()
        && cli.base.is_none()
        && cli.apply_patch.is_none()
        && left.is_file()
        && right.is_file()
    {
        let left_kind = diff_core::detect::detect_file(&left);
        let right_kind = diff_core::detect::detect_file(&right);
        let left_code = matches!(left_kind, FileKind::Executable | FileKind::Dex)
            || (matches!(left_kind, FileKind::Zip)
                && diff_core::disassembly::archive_contains_code(&left));
        let right_code = matches!(right_kind, FileKind::Executable | FileKind::Dex)
            || (matches!(right_kind, FileKind::Zip)
                && diff_core::disassembly::archive_contains_code(&right));
        if left_code || right_code {
            let tool = match &cli.objdump {
                Some(path) if path.is_file() => Some(path.clone()),
                Some(path) => anyhow::bail!("objdump not found at {}", path.display()),
                None => None,
            };
            if confirm_decompile(&left, &right, tool.as_deref(), cli.yes)? {
                options.decompile = Some(diff_core::engine::DecompileOptions {
                    objdump: tool,
                    max_lines: 20_000,
                });
            }
        }
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
            max_pages: cli.pdf_max_pages,
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

    if cli.pdf_visual {
        let pdftoppm = resolve_pdftoppm(cli.pdftoppm.as_deref())?;
        let output_dir = cli
            .pdf_output_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("pdf-visual-output"));
        let visual_options = diff_core::pdf_visual::PdfVisualOptions {
            pdftoppm,
            dpi: cli.pdf_dpi,
            max_pages: cli.pdf_max_pages,
        };
        let report =
            diff_core::pdf_visual::visual_pdf_diff(&left, &right, &output_dir, &visual_options)
                .with_context(|| "PDF visual diff failed")?;
        if !cli.quiet {
            let text = if cli.format == Format::Json {
                serde_json::to_string_pretty(&report)?
            } else {
                report.summary_text()
            };
            emit(&text, &cli.output)?;
        }
        return Ok(if report.has_changes() { 1 } else { 0 });
    }

    let engine = DiffEngine::new(options);
    let mut report = engine.compare(&left, &right).with_context(|| {
        format!(
            "failed to compare {} and {}",
            left.display(),
            right.display()
        )
    })?;
    report.left.path = left_display;
    report.right.path = right_display;

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
    if cli.detail == DetailMode::Concise {
        diff_core::concise::apply(&mut report);
    }

    if cli.quiet {
        return Ok(exit_code(report.status));
    }

    if cli.name_only {
        let text = changed_paths_text(&report);
        emit(&text, &cli.output)?;
        return Ok(exit_code(report.status));
    }

    let render_options = RenderOptions {
        color: !cli.no_color
            && cli.output.is_none()
            && std::env::var_os("NO_COLOR").is_none()
            && io::stdout().is_terminal(),
        summary_only: cli.summary_only,
    };

    if cli.format == Format::Tui && cli.output.is_none() && io::stdout().is_terminal() {
        let ansi = render::ansi::render(
            &report,
            &RenderOptions {
                color: true,
                summary_only: cli.summary_only,
            },
        );
        tui::run("DiffEverything", &ansi)?;
        return Ok(exit_code(report.status));
    }

    let rendered = match cli.format {
        Format::Ansi | Format::Tui => render::ansi::render(&report, &render_options),
        Format::Json => render::json::render(&report),
        Format::Markdown => render::markdown::render(&report),
        Format::Html => render::html::render(&report),
        Format::Unified => render::unified::render(&report),
    };
    emit(&rendered, &cli.output)?;
    Ok(exit_code(report.status))
}

fn exit_code(status: DiffStatus) -> i32 {
    if status == DiffStatus::Equal { 0 } else { 1 }
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
                "tesseract was not found. Download the portable build with scripts/fetch-testdata.ps1, put tesseract on PATH, set TESSERACT_PATH, or pass --tesseract"
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
                report.left.path, report.right.path
            ));
        } else {
            out.push_str(&format!("{marker} {path}\n"));
        }
    }
    out
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
