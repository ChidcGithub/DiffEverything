use serde::Serialize;
use std::path::Path;

pub const TOOL_NAME: &str = "DiffEverything";
pub const TOOL_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (Toyger)");
pub const COMPLETELY_DIFFERENT_THRESHOLD: f64 = 0.90;

/// Broad category of an input artifact. Handlers are selected from this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Text,
    Json,
    Yaml,
    Toml,
    Ini,
    Xml,
    Html,
    Plist,
    Pickle,
    Mo,
    Wasm,
    JavaClass,
    AndroidXml,
    GitIndex,
    Ico,
    SshPublicKey,
    Dex,
    Sqlite,
    Csv,
    /// Internal kind used when two different structured formats are crossed
    /// (for example JSON vs YAML).
    Structured,
    Notebook,
    Image,
    Audio,
    Font,
    Binary,
    Executable,
    Directory,
    Zip,
    SevenZip,
    Tar,
    Deb,
    Rpm,
    Iso,
    Cpio,
    Cab,
    Xar,
    Ar,
    Gzip,
    Zstd,
    Bzip2,
    Xz,
    Lz4,
    Pdf,
    Ofd,
    Docx,
    Xlsx,
    Pptx,
    Office,
    Squashfs,
    Snap,
    AppImage,
    Flatpak,
    Dmg,
    Msix,
    Aab,
    Ext4,
    Parquet,
    Avro,
    Orc,
    Cbor,
    MessagePack,
    Protobuf,
    Odt,
    Odp,
    Epub,
    Rtf,
    Eml,
    Video,
    GitHistory,
    Unknown,
}

impl FileKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Json => "json",
            Self::Yaml => "yaml",
            Self::Toml => "toml",
            Self::Ini => "ini",
            Self::Xml => "xml",
            Self::Html => "html",
            Self::Plist => "plist",
            Self::Pickle => "pickle",
            Self::Mo => "gettext",
            Self::Wasm => "wasm",
            Self::JavaClass => "java-class",
            Self::AndroidXml => "android-xml",
            Self::GitIndex => "git-index",
            Self::Ico => "ico",
            Self::SshPublicKey => "openssh-key",
            Self::Dex => "dex",
            Self::Sqlite => "sqlite",
            Self::Csv => "csv",
            Self::Structured => "structured",
            Self::Notebook => "notebook",
            Self::Image => "image",
            Self::Audio => "audio",
            Self::Font => "font",
            Self::Binary => "binary",
            Self::Executable => "executable",
            Self::Directory => "directory",
            Self::Zip => "zip",
            Self::SevenZip => "7z",
            Self::Tar => "tar",
            Self::Deb => "deb",
            Self::Rpm => "rpm",
            Self::Iso => "iso",
            Self::Cpio => "cpio",
            Self::Cab => "cab",
            Self::Xar => "xar",
            Self::Ar => "ar",
            Self::Gzip => "gzip",
            Self::Zstd => "zstd",
            Self::Bzip2 => "bzip2",
            Self::Xz => "xz",
            Self::Lz4 => "lz4",
            Self::Pdf => "pdf",
            Self::Ofd => "ofd",
            Self::Docx => "docx",
            Self::Xlsx => "xlsx",
            Self::Pptx => "pptx",
            Self::Office => "office",
            Self::Squashfs => "squashfs",
            Self::Snap => "snap",
            Self::AppImage => "appimage",
            Self::Flatpak => "flatpak",
            Self::Dmg => "dmg",
            Self::Msix => "msix",
            Self::Aab => "aab",
            Self::Ext4 => "ext4",
            Self::Parquet => "parquet",
            Self::Avro => "avro",
            Self::Orc => "orc",
            Self::Cbor => "cbor",
            Self::MessagePack => "messagepack",
            Self::Protobuf => "protobuf",
            Self::Odt => "odt",
            Self::Odp => "odp",
            Self::Epub => "epub",
            Self::Rtf => "rtf",
            Self::Eml => "eml",
            Self::Video => "video",
            Self::GitHistory => "git-history",
            Self::Unknown => "unknown",
        }
    }

    pub fn is_textual(self) -> bool {
        matches!(
            self,
            Self::Text
                | Self::Json
                | Self::Yaml
                | Self::Toml
                | Self::Ini
                | Self::Xml
                | Self::Html
                | Self::Csv
                | Self::Structured
                | Self::Notebook
        )
    }

    pub fn is_structured(self) -> bool {
        matches!(
            self,
            Self::Json
                | Self::Yaml
                | Self::Toml
                | Self::Ini
                | Self::Xml
                | Self::Html
                | Self::Plist
                | Self::Pickle
                | Self::Sqlite
                | Self::Notebook
                | Self::Parquet
                | Self::Avro
                | Self::Orc
                | Self::Cbor
                | Self::MessagePack
                | Self::Protobuf
                | Self::Rtf
                | Self::Eml
                | Self::GitHistory
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffStatus {
    Equal,
    Different,
    Unsupported,
    Error,
}

impl DiffStatus {
    pub fn is_different(self) -> bool {
        !matches!(self, Self::Equal)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Equal => "EQUAL",
            Self::Different => "DIFFERENT",
            Self::Unsupported => "UNSUPPORTED",
            Self::Error => "ERROR",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtifactInfo {
    pub path: String,
    pub kind: FileKind,
    pub size: u64,
}

impl ArtifactInfo {
    pub fn from_path(path: &Path, kind: FileKind) -> Self {
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        Self {
            path: path.display().to_string(),
            kind,
            size,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Summary {
    pub files_compared: usize,
    pub files_added: usize,
    pub files_removed: usize,
    pub files_changed: usize,
    pub files_equal: usize,
    pub lines_added: usize,
    pub lines_removed: usize,
    pub bytes_differing: u64,
    pub structured_changes: usize,
    pub table_rows_added: usize,
    pub table_rows_removed: usize,
    pub table_rows_changed: usize,
    pub table_cells_changed: usize,
    pub workbook_cells_changed: usize,
    pub tokens_added: usize,
    pub tokens_removed: usize,
    pub tree_added: usize,
    pub tree_removed: usize,
    pub tree_modified: usize,
    pub tree_moved: usize,
    pub notebook_cells_added: usize,
    pub notebook_cells_removed: usize,
    pub notebook_cells_changed: usize,
    pub notebook_cells_moved: usize,
    pub audio_tag_changes: usize,
    pub audio_property_changes: usize,
}

impl Summary {
    pub fn merge(&mut self, other: &Summary) {
        self.files_compared += other.files_compared;
        self.files_added += other.files_added;
        self.files_removed += other.files_removed;
        self.files_changed += other.files_changed;
        self.files_equal += other.files_equal;
        self.lines_added += other.lines_added;
        self.lines_removed += other.lines_removed;
        self.bytes_differing += other.bytes_differing;
        self.structured_changes += other.structured_changes;
        self.table_rows_added += other.table_rows_added;
        self.table_rows_removed += other.table_rows_removed;
        self.table_rows_changed += other.table_rows_changed;
        self.table_cells_changed += other.table_cells_changed;
        self.workbook_cells_changed += other.workbook_cells_changed;
        self.tokens_added += other.tokens_added;
        self.tokens_removed += other.tokens_removed;
        self.tree_added += other.tree_added;
        self.tree_removed += other.tree_removed;
        self.tree_modified += other.tree_modified;
        self.tree_moved += other.tree_moved;
        self.notebook_cells_added += other.notebook_cells_added;
        self.notebook_cells_removed += other.notebook_cells_removed;
        self.notebook_cells_changed += other.notebook_cells_changed;
        self.notebook_cells_moved += other.notebook_cells_moved;
        self.audio_tag_changes += other.audio_tag_changes;
        self.audio_property_changes += other.audio_property_changes;
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffReport {
    pub tool: &'static str,
    pub version: &'static str,
    pub left: ArtifactInfo,
    pub right: ArtifactInfo,
    pub status: DiffStatus,
    pub summary: Summary,
    pub root: DiffNode,
    pub warnings: Vec<String>,
    pub elapsed_ms: u128,
    /// True when more than 90% of the compared content changed.
    #[serde(default)]
    pub completely_different: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decompiled: Option<DecompilationDiff>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DecompilationDiff {
    pub tool: String,
    pub left_lines: usize,
    pub right_lines: usize,
    pub truncated: bool,
    pub text: TextDiff,
}

impl DiffReport {
    pub fn has_differences(&self) -> bool {
        self.status.is_different()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffNode {
    pub label: String,
    pub status: DiffStatus,
    pub body: DiffBody,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decompiled: Option<DecompilationDiff>,
}

impl DiffNode {
    pub fn new(label: impl Into<String>, status: DiffStatus, body: DiffBody) -> Self {
        Self {
            label: label.into(),
            status,
            body,
            decompiled: None,
        }
    }

    pub fn with_decompiled(mut self, decompiled: DecompilationDiff) -> Self {
        self.decompiled = Some(decompiled);
        self
    }
}

/// A node in the comparison tree. Directories and archives contain children;
/// leaf nodes carry format-specific detail.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DiffBody {
    Text(TextDiff),
    Tokens(TokenDiff),
    Tree(TreeDiff),
    Notebook(NotebookDiff),
    Structured(StructuredDiff),
    Table(TableDiff),
    Workbook(WorkbookDiff),
    Image(ImageDiff),
    Audio(AudioDiff),
    Binary(BinaryDiff),
    Directory(DirectoryDiff),
    Archive(ArchiveDiff),
    OneSided(OneSidedInfo),
    Equal(EqualInfo),
    Unsupported(UnsupportedInfo),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TextTag {
    Equal,
    Delete,
    Insert,
}

#[derive(Debug, Clone, Serialize)]
pub struct TextDiff {
    pub format: String,
    pub encoding_left: String,
    pub encoding_right: String,
    #[serde(default)]
    pub left_lines: usize,
    #[serde(default)]
    pub right_lines: usize,
    pub added_lines: usize,
    pub removed_lines: usize,
    pub hunks: Vec<TextHunk>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TextHunk {
    pub old_start: usize,
    pub old_len: usize,
    pub new_start: usize,
    pub new_len: usize,
    pub lines: Vec<TextLine>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TextLine {
    pub tag: TextTag,
    pub old_lineno: Option<usize>,
    pub new_lineno: Option<usize>,
    pub text: String,
    /// Word-level marking for modified lines. Empty for unchanged lines or
    /// when the line is too large to annotate.
    pub spans: Vec<InlineSpan>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InlineSpan {
    pub text: String,
    pub changed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenDiff {
    pub language: String,
    pub added_tokens: usize,
    pub removed_tokens: usize,
    pub chunks: Vec<TokenChunk>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenChunk {
    pub tag: TextTag,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TreeDiff {
    pub language: String,
    pub added_nodes: usize,
    pub removed_nodes: usize,
    pub modified_nodes: usize,
    pub moved_nodes: usize,
    pub chunks: Vec<TreeChunk>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TreeChunk {
    pub tag: TreeTag,
    pub path: String,
    pub text: String,
    pub depth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TreeTag {
    Added,
    Removed,
    Modified,
    Moved,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotebookDiff {
    pub left_cells: usize,
    pub right_cells: usize,
    pub cells_added: usize,
    pub cells_removed: usize,
    pub cells_moved: usize,
    pub changed_cells: Vec<NotebookCellDiff>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotebookCellDiff {
    pub index: usize,
    pub cell_type: String,
    pub source: Option<TextDiff>,
    pub outputs_changed: bool,
    pub execution_changed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructuredDiff {
    pub format: String,
    pub changes: Vec<StructuredChange>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructuredChange {
    pub path: String,
    pub kind: StructuredChangeKind,
    pub left: Option<serde_json::Value>,
    pub right: Option<serde_json::Value>,
    #[serde(default)]
    pub breaking: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StructuredChangeKind {
    Added,
    Removed,
    Changed,
}

#[derive(Debug, Clone, Serialize)]
pub struct TableDiff {
    pub format: String,
    pub delimiter: char,
    pub left_rows: usize,
    pub right_rows: usize,
    pub columns_added: Vec<String>,
    pub columns_removed: Vec<String>,
    pub rows: Vec<TableRowChange>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TableRowChange {
    pub key: String,
    pub kind: RowChangeKind,
    pub cells: Vec<CellChange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowChangeKind {
    Added,
    Removed,
    Changed,
}

#[derive(Debug, Clone, Serialize)]
pub struct CellChange {
    pub column: String,
    pub left: Option<String>,
    pub right: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkbookDiff {
    pub format: String,
    pub sheets_added: Vec<String>,
    pub sheets_removed: Vec<String>,
    pub sheets: Vec<SheetDiff>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SheetDiff {
    pub name: String,
    pub left_rows: u32,
    pub left_columns: u32,
    pub right_rows: u32,
    pub right_columns: u32,
    pub cells: Vec<CellChange>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageDiff {
    pub left_width: u32,
    pub left_height: u32,
    pub right_width: u32,
    pub right_height: u32,
    pub differing_pixels: u64,
    pub total_pixels: u64,
    pub diff_ratio: f64,
    pub max_channel_delta: u8,
    pub bounding_box: Option<BoundingBox>,
    pub regions: Vec<BoundingBox>,
    pub regions_truncated: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct BoundingBox {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioDiff {
    pub left_format: String,
    pub right_format: String,
    pub left_duration_ms: u128,
    pub right_duration_ms: u128,
    pub left_bitrate: Option<u32>,
    pub right_bitrate: Option<u32>,
    pub left_sample_rate: Option<u32>,
    pub right_sample_rate: Option<u32>,
    pub left_channels: Option<u8>,
    pub right_channels: Option<u8>,
    pub tags: Vec<AudioTagChange>,
    pub content_differs: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waveform: Option<WaveformDiff>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WaveformDiff {
    pub sample_rate: u32,
    pub channels: usize,
    pub window_ms: u32,
    pub left: Vec<f32>,
    pub right: Vec<f32>,
    pub max_delta: f32,
    pub changed_windows: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioTagChange {
    pub key: String,
    pub left: Option<String>,
    pub right: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BinaryDiff {
    pub left_len: u64,
    pub right_len: u64,
    pub differing_bytes: u64,
    pub first_difference: Option<u64>,
    pub rows: Vec<HexRow>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct HexRow {
    pub offset: u64,
    pub left_hex: String,
    pub right_hex: String,
    pub left_ascii: String,
    pub right_ascii: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DirectoryDiff {
    pub entries: Vec<DiffNode>,
    pub total_entries: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArchiveDiff {
    pub format: String,
    pub entries: Vec<DiffNode>,
    pub total_entries: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PresenceSide {
    LeftOnly,
    RightOnly,
}

#[derive(Debug, Clone, Serialize)]
pub struct OneSidedInfo {
    pub side: PresenceSide,
    pub kind: FileKind,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct EqualInfo {
    pub kind: FileKind,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnsupportedInfo {
    pub note: String,
}
