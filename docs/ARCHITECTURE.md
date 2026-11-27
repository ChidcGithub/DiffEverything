# DiffEverything 架构

## 模块

```text
crates/
├─ diff-core/
│  ├─ src/detect.rs          扩展名 + magic bytes + 文本启发式
│  ├─ src/model.rs           报告数据模型（DiffReport / DiffNode / DiffBody）
│  ├─ src/engine.rs          编排：读取、分派、递归、统计、限制
│  ├─ src/merge.rs           文本三方合并
│  ├─ src/patch.rs           bsdiff 补丁生成/还原
│  ├─ src/image_export.rs    热力图、并排图
│  ├─ src/ocr.rs             Tesseract 图片/PDF OCR
│  ├─ src/pdf_visual.rs      poppler 页面渲染、逐页视觉 diff、HTML/PDF 报告
│  ├─ src/session.rs         会话保存/加载
│  ├─ src/sync.rs            目录同步计划/应用/备份
│  ├─ src/diff/
│  │  ├─ text.rs             编码解码、行级 diff、word-level spans、忽略空白/大小写
│  │  ├─ tokens.rs           tree-sitter token diff
│  │  ├─ tree_diff.rs        tree-sitter 语法树节点递归 diff
│  │  ├─ notebook.rs         cell 级 diff / 三方合并
│  │  ├─ structured.rs       JSON/JSON5/YAML/TOML/INI/XML/HTML 语义 diff
│  │  ├─ table.rs            CSV/TSV 表格 diff
│  │  ├─ workbook.rs         XLSX/XLS/ODS 单元格 diff
│  │  ├─ documents.rs        PDF/OFD/DOCX/PPTX 文本提取
│  │  ├─ image.rs            像素级 diff
│  │  └─ binary.rs           Hex / 字节级 diff
│  ├─ src/render/
│  │  ├─ ansi.rs             终端报告
│  │  ├─ json.rs             JSON 报告
│  │  ├─ markdown.rs         Markdown 报告
│  │  ├─ html.rs             自包含 HTML 报告
│  │  └─ unified.rs          GNU unified diff
│  └─ tui.rs                 ratatui 交互式查看器
│  └─ tests/                 engine / formats / robustness
└─ diff-cli/
   ├─ src/main.rs            参数解析、退出码、merge/patch/image 分支
   └─ tests/cli.rs           CLI 端到端测试
```

## 数据流

```text
Path / bytes
    │
    ▼
detect_file / detect_bytes        FileKind
    │
    ▼
read + size guard                 Vec<u8> / streaming
    │
    ▼
normalize                         text / JSON tree / pixels / table / workbook / document text / bytes
    │
    ▼
compare                           DiffBody
    │
    ▼
DiffNode tree + Summary
    │
    ▼
renderer                          ANSI / JSON / Markdown / HTML / unified
```

核心原则与 diffoscope 类似：**检测 → 归一化 → 比较 → 呈现**。把“格式解析”和“差异呈现”分开，才能在不改渲染器的前提下增加格式。

## 核心模型

`DiffReport` 是渲染器唯一输入：

```rust
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
}
```

`DiffNode` 是递归树：

```rust
pub struct DiffNode {
    pub label: String,      // 目录/归档内是相对路径；根节点为空
    pub status: DiffStatus,
    pub body: DiffBody,
}
```

`DiffBody` 变体：

| 变体 | 含义 |
|---|---|
| `Text(TextDiff)` | 文本/文档/OCR 行级 diff，含行内 `InlineSpan` |
| `Tokens(TokenDiff)` | tree-sitter token 序列 diff |
| `Tree(TreeDiff)` | tree-sitter 语法树节点 diff |
| `Notebook(NotebookDiff)` | Notebook cell 级 diff / 移动 |
| `Audio(AudioDiff)` | 音频属性与标签 |
| `Structured(StructuredDiff)` | JSON/YAML/TOML/INI/XML 路径级 diff |
| `Table(TableDiff)` | CSV/TSV 行/列/单元格 diff |
| `Workbook(WorkbookDiff)` | 工作表/单元格 diff |
| `Image(ImageDiff)` | 像素统计和包围盒 |
| `Binary(BinaryDiff)` | 字节统计和 Hex 行 |
| `Directory(DirectoryDiff)` | 目录容器 |
| `Archive(ArchiveDiff)` | ZIP/TAR 容器 |
| `OneSided` | 只存在于一侧 |
| `Equal` | 相同 |
| `Unsupported` | 暂不支持 |

设计约束：

- 渲染器不读文件、不做类型判断；
- 明细数字（行数、字节数、像素数、单元格数）在比较阶段算好；
- 容器节点状态由子节点聚合；
- warning 放报告顶层，避免重复；
- 所有格式共享大小/递归/条目限制。

## 新增格式的步骤

以 7z 为例：

1. `detect.rs`：扩展名/ magic → `FileKind::SevenZip`。
2. `diff/`：新增解析器，把 7z 解为 `BTreeMap<String, Vec<u8>>`。
3. `engine.rs`：在 `compare_bytes_internal` 中调用通用 `compare_archive_entries`。
4. `model.rs`：如果复用 `ArchiveDiff`，无需新增 `DiffBody`。
5. 渲染器：如果复用 `Archive`，无需修改。
6. 测试：条目差异、损坏归档回退、条目上限。

新增叶子格式（例如 OCR 文本）时，优先复用 `TextDiff` / `StructuredDiff`，避免为每个格式重写渲染逻辑。

## 文本 diff

1. `decode_text`：BOM → UTF-8 → chardetng 统计检测（支持 UTF-16/GBK）。
2. `compare_text_options`：
   - 可选忽略大小写、空白；
   - 用 normalized 文本做匹配；
   - 渲染时回填原始行，避免显示被规范化后的内容。
3. `annotate_word_changes`：对连续删除/插入行做 word-level diff，生成 `InlineSpan`。
4. 输出保留 hunk 行号与上下文。

## 语法 token / tree diff

- 根据文件扩展名选择 tree-sitter `Language`（13 种：Rust/Python/JS/TS/C/C++/Go/Java/Bash/Ruby/C#/Swift/Lua）；
- token 模式：递归收集叶子节点，Myers 比较 token 序列，生成 `TokenChunk`；
- tree 模式：递归构建 `SyntaxNode`，按节点 kind/子节点签名对齐，生成 added/removed/modified 节点；
- Notebook：按 cell id/签名对齐，检测 moved cells，三方合并按 id 对齐并保留 ours 的移动顺序；
- `--ignore-comments` 跳过 `*comment*` 子树；
- 解析失败/超限自动回退行级文本 diff。

## PDF 视觉与 OCR

- `--pdf-visual` 调用 poppler `pdftoppm` 逐页渲染 PNG；
- 复用 `diff-core/src/diff/image.rs` 计算像素差异；
- 生成 `heatmaps/`、`side-by-side/`、`overlays/`、`report.html`、`report.pdf`、栅格叠加 `report-overlay.pdf`，以及通过 lopdf 在原 PDF 内容流追加矢量红色矩形的 `report-vector.pdf`；
- `--ocr` 对图片调用 Tesseract；对 PDF 先用 `pdftoppm` 渲染页面，再逐页 OCR，最后走文本 diff；
- 工具路径可通过 `--tesseract` / `--pdftoppm` / 环境变量指定，portable 版本由 `scripts/fetch-testdata.ps1` 下载。

## 远程输入

- `webdav://` / `webdavs://` 映射到 HTTP(S)，`http(s)://` 直接 GET，`sftp://` 通过 ssh2 读取；
- URL 用户信息或 `--remote-user/--remote-password` 提供认证；
- 文件下载到 `tempfile::TempDir`，比较期间保持存活，结束后自动清理；
- 报告中的路径保留原始 URL；会话保存的是原始 URL。
- 测试使用 tiny_http 本地 WebDAV/HTTP 服务器和 `scripts/sftp-test-server.py`（asyncssh）本地 SFTP 服务器。

## 目录同步与会话

- `sync` 先遍历左右目录，按内容一致性生成 Copy/Delete 计划；
- 默认只打印计划，`--sync-apply` 才执行；覆盖/删除前可写入 `--backup-dir`；
- 镜像模式会删除目标端多余文件并清理空目录；
- `--save-session` 保存路径和完整 `DiffOptions`，`--load-session` 恢复后直接比较。

## 文档格式

- PDF：`pdf-extract` 取文本，`lopdf` 取页数；失败回退二进制。
- OFD：ZIP 读取 `Doc_*/Pages/*/Content.xml`，提取 `TextCode` 文本。
- 音频：lofty 读取 format/duration/bitrate/sample rate/channels/tags，比较元数据，同时检查内容是否变化。
- DOCX：zip 读 `word/document.xml`、`header*.xml`、`footer*.xml`、脚注/尾注/评论；`quick-xml` 提取段落和表格文本。
- PPTX：zip 读 `ppt/slides/slideN.xml` 和 `notesSlides/notesSlideN.xml`；提取 `<a:t>`。
- XLSX/XLS/ODS：calamine 读取工作表与单元格，按地址比较。
- 这些格式最终复用 `TextDiff` / `WorkbookDiff`，所以报告格式一致。

## 渲染策略

- **ANSI**：本地 CLI 和 CI 日志；`NO_COLOR` / 非 TTY 自动关闭颜色。
- **JSON**：稳定的机器接口，CLI 测试会解析验证。
- **Markdown**：PR 评论和文档归档。
- **HTML**：自包含单文件，便于分享。
- **unified**：兼容 `diff`/`patch` 工作流。

## 性能与安全边界

- 小于 `max_file_bytes` 的文件一次性读入；
- 超过阈值走 64 KiB 分块流式比较，只输出摘要；
- 目录先收集文件表再比较，便于统计和截断；
- ZIP/TAR 在内存中读取，单条目有上限，避免 zip bomb；
- 不跟随符号链接；
- 所有限制通过 `DiffOptions` 暴露。
