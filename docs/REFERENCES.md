# 参考项目与实现映射

本项目的实现基于对以下开源项目的**功能与算法学习**。参考仓库已浅克隆到 `references/`（被 `.gitignore` 忽略），不参与编译，也不会把第三方代码复制进 `diff-core`。

## 仓库版本

| 项目 | 本地路径 | Commit | License |
|---|---|---|---|
| diffoscope | `references/diffoscope` | `dcfffcb` | GPL-3.0 |
| 7-Zip / sevenz-rust | crates.io / `testdata/tools/7zr.exe` | - | LGPL / MIT |
| Graphtage | `references/graphtage` | `1f6688d` | LGPL-3.0 |
| difftastic | `references/difftastic` | `4b834ef` | MIT |
| Open Diff | `references/open-diff` | `f42cd1a` | Apache-2.0 |
| uni-diff | `references/uni-diff` | `8418e97` | MIT |
| diff-pdf | `references/diff-pdf` | `092e6e3` | GPL-2.0 |
| nbdime | `references/nbdime` | `744c978` | BSD-3-Clause |
| HDiffPatch | `references/HDiffPatch` | `0696136` | MIT |
| gocompare | `references/gocompare` | `79539f5` | MIT |
| doc_diff_software | `references/doc-diff-software` | `e3e3f81` | 未在仓库中声明 |
| Meld | `references/meld` | `35d6164` | GPL-2.0 |
| KDiff3 | `references/kdiff3` | `6c4eb7e` | GPL-2.0 |
| diff-so-fancy | `references/diff-so-fancy` | `cb21bb0` | MIT |
| jsdiff | `references/jsdiff` | `7aac04a` | BSD-3-Clause |
| diff-match-patch | `references/diff-match-patch` | `62f2e68` | Apache-2.0 |
| pixelmatch | `references/pixelmatch` | `40558aa` | ISC |
| openapi-diff | `references/openapi-diff` | `594183e` | Apache-2.0 |
| csvdiff | `references/csvdiff` | `1007bf3` | MIT |
| daff | `references/daff` | `ad5a02e` | MIT |
| diffsitter | `references/diffsitter` | `a5a6681` | MIT |

> GPL / LGPL 项目只用于阅读设计，没有复制代码。DiffEverything 的 Rust 实现为独立实现。

## 补充参考项目

- **Meld / KDiff3**：目录/文件三方合并与 GUI 工作流，用于校验 CLI merge/sync 语义。
- **diff-so-fancy**：ANSI 人类可读 diff 的布局与颜色。
- **jsdiff / diff-match-patch**：文本 diff 算法与字符级 diff 思路。
- **pixelmatch**：图片像素容差和差异计数。
- **openapi-diff**：OpenAPI 文档的语义 diff；DiffEverything 自动识别 OpenAPI，并在结构化 diff 上标记 breaking change。
- **csvdiff / daff**：CSV 表格 diff、按主键对齐与单元格差异。
- **diffsitter**：tree-sitter 结构 diff；与本项目 token/tree 模式互为参考。

## 具体借鉴点

### diffoscope（递归比较与报告分层）

阅读文件：

- `references/diffoscope/diffoscope/comparators/__init__.py`
- `references/diffoscope/diffoscope/comparators/utils/file.py`
- `references/diffoscope/diffoscope/presenters/`
- `references/diffoscope/diffoscope/main.py`

借鉴：

- 按文件类型选择 comparator；
- 容器格式递归解包后继续按条目类型分派；
- 比较结果与呈现器解耦；
- 文本 / HTML / JSON 多种报告；
- 递归深度、条目数量、文件大小等资源限制。

DiffEverything 对应：

- `crates/diff-core/src/engine.rs`：`compare_bytes_internal` 分派、ZIP/7Z/TAR（gz/bz2/xz/zst/lz4）/CPIO/DEB/RPM/ISO 递归、单文件 gz/zst/bz2/xz/lz4 解压、目录递归；
- `crates/diff-core/src/model.rs`：`DiffReport` / `DiffNode` / `DiffBody`；
- `crates/diff-core/src/render/`：ANSI / JSON / Markdown / HTML / unified；
- `DiffOptions`：`max_file_bytes`、`max_dir_entries`、`max_zip_entries`、`max_depth`。

已实现 DEB（ar 容器 → control/data tar → 条目类型递归分派）、RPM（元数据 + gzip/xz/zstd payload）、ISO9660 目录树与 CPIO newc 递归、CAB（MSZIP/LZX）、XAR（TOC zlib + zlib/bzip2/xz/lzma/raw 条目）、AR（GNU/BSD 长名、符号表）与 TAR symlink/hardlink 递归、SQLite 表/列/行/schema 语义比较、ELF/PE/Mach-O 可执行文件节/符号/导入语义比较、TTF/OTF/TTC/WOFF/WOFF2 字体元数据与 advance 比较、WebAssembly 模块类型/导入导出/代码段比较、Java class 字段/方法/bytecode 比较、Android 二进制 XML manifest 比较、Git index 条目比较、ICO/CUR 容器比较、IPK 包递归比较、OpenSSH 公钥字段比较、APK 内嵌 DEX/.so 反编译、通用外部工具 comparator（`--external-tool`）。PDF 页面视觉 diff 已通过可选 poppler 渲染实现，`report.html` 内嵌 base64 PNG。

### Graphtage（结构化语义树与跨格式）

阅读文件：

- `references/graphtage/graphtage/__main__.py`
- `references/graphtage/graphtage/edits.py`
- `references/graphtage/graphtage/json.py`
- `references/graphtage/graphtage/csv.py`
- `references/graphtage/graphtage/ini.py`
- `references/graphtage/graphtage/xml.py`
- `references/graphtage/graphtage/yaml.py`

借鉴：

- 将不同格式解析成统一树，再生成路径级 edit；
- 允许不同输入格式互相比较；
- 输出保持稳定的路径表示。

DiffEverything 对应：

- `crates/diff-core/src/diff/structured.rs`：JSON / JSON5 / YAML / TOML / INI / XML / HTML → `serde_json::Value`，统一 `compare_values`；HTML 使用 scraper 构建 DOM 树；
- `FileKind::Structured` 与 `effective_kind`：JSON vs YAML 等跨格式；
- `render/ansi.rs` / `markdown.rs` / `html.rs`：路径级 `+/-/~` 输出；
- `--format json` 暴露结构化 edit 列表。

已实现 plist（XML 与 binary）与 pickle（protocol 2-5），均可与 JSON/YAML 等跨格式比较。数组使用相似度动态规划对齐，插入/删除不会把后续元素全部误报为修改；大数组回退 Myers。Graphtage 的完整编辑距离图搜索未逐行复刻。HTML DOM 已通过 scraper 解析为统一树。

### difftastic（tree-sitter 语法感知）

阅读文件：

- `references/difftastic/src/parse/tree_sitter_parser.rs`
- `references/difftastic/src/diff/tree_diff.rs`
- `references/difftastic/src/options.rs`
- `references/difftastic/src/words.rs`

借鉴：

- tree-sitter 解析源码，按语法叶子取 token；
- 忽略注释选项；
- 语法级比较与退出码约定。

DiffEverything 对应：

- `crates/diff-core/src/diff/tokens.rs`：13 种语言的 `Language` 映射、叶节点 token 收集、`--ignore-comments`；
- `compare_token_streams`：Myers 算法比较 token 序列；
- `crates/diff-core/src/diff/tree_diff.rs`：构建 tree-sitter 语法树，按节点类型/子节点签名递归对齐，生成 added/removed/modified 节点；
- `DiffBody::Tokens` / `DiffBody::Tree` + 五个 renderer；
- `--mode token` / `--mode tree`，以及 `--ignore-comments` 自动启用 token 模式。

差异：tree mode 使用内容签名、最优二分匹配和顺序滑动归一化，超大节点回退 Myers；graph mode 将语法树线性化为 enter/token/exit 事件并运行有界最短路径，token 按文本跨深度等价、按节点类型计价，整棵增删/高比例修改子树会分组输出，精确移动子树与稳定子树作为非局部零代价锚点加入对齐图，并用 LCS（最小索引位移滑块）检测移动子树（含移动后再修改）输出 `Moved` 标记，但未逐行复刻 difftastic 的完整 Dijkstra 图搜索与全图滑块代价。支持语言为 Rust/Python/JS/TS/C/C++/Go/Java/Bash/Ruby/C#/Swift/Lua。

### Open Diff（工作流与多模式）

阅读文件：

- `references/open-diff/README.md`
- `references/open-diff/src/api/diff.ts`
- `references/open-diff/src/api/folderMerge.ts`
- `references/open-diff/src/api/sync.ts`
- `references/open-diff/src/api/script.ts`

借鉴：

- 文本 / 表格 / 图片 / Hex / 二进制 / 目录统一工作流；
- 三方合并（文本、Notebook、目录）；
- HTML 报告、忽略空白/大小写/注释、排除规则；
- CLI 退出码与脚本化。

DiffEverything 对应：

- 全部对应格式的 CLI 模式；
- `diff-core/src/merge.rs`（diffy 三方合并）；
- `diff-core/src/folder_merge.rs`（目录三方合并计划/应用）；
- `--ignore-whitespace`、`--ignore-case`、`--ignore-comments`、`--exclude`；
- `--format html`、`--detail concise|full`、`-y`/`--objdump` 反编译授权、`--head` / `--last` / `--filter`、`--name-only`、`--quiet`、`--summary-only`。

未实现：GUI。目录同步（计划/应用/备份）、会话保存/加载、WebDAV/SFTP/SMB 远程文件均已实现。

### uni-diff（文档格式与输出）

阅读文件：

- `references/uni-diff/README.md`
- `references/uni-diff/uni_diff/`（converter / renderer 结构）

借鉴：

- PDF / DOCX / XLSX / PPTX 的文本化思路；
- ANSI / JSON / HTML / PNG / TUI 多输出；
- 块级 diff 与定位信息。

DiffEverything 对应：

- `documents.rs`：PDF 文本 + 页数、DOCX 正文/页眉页脚、PPTX 幻灯片/备注；
- `workbook.rs`：XLSX/XLS/ODS 单元格；
- `render/`：ANSI/JSON/Markdown/HTML/unified；
- `image_export.rs`：热力图、并排图（PNG）。

已实现：OCR（图片 + 渲染后的 PDF 页面）、PDF 页面渲染 PNG 热力图/并排图/HTML 报告、PDF 文本页级定位（`# Page N`）、图片块级 changed regions、交互式 TUI（管道时回退 ANSI）。未实现：文档内字符/坐标级 block-level 视觉定位。

### diff-pdf（PDF 视觉比较）

阅读文件：`references/diff-pdf/diff-pdf.cc`。

借鉴：先渲染页面为位图再逐页像素比较的思想；退出码与 `--output-diff` 的产品化设计。

DiffEverything 对应：PDF 文本层 + 页数 diff；`--pdf-visual` 使用 poppler `pdftoppm` 渲染页面，生成逐页像素统计、热力图、并排图和 HTML 报告；`--ocr` 可对渲染页做 OCR。

已实现：`--pdf-visual` 输出 `report.pdf`（并排+热力图）、`report-overlay.pdf`（原页面栅格红色叠加）、`report-vector.pdf`（用 lopdf 在原页面内容流追加红色矢量矩形）、`report.html`（内嵌 base64 PNG）和 PNG。

### nbdime（Notebook）

阅读文件：

- `references/nbdime/nbdime/diffing/notebooks.py`
- `references/nbdime/nbdime/diff_format.py`
- `references/nbdime/nbdime/merging/`

借鉴：Notebook 本质是 JSON，但 cell/source/output 需要稳定路径。

DiffEverything 对应：

- `crates/diff-core/src/diff/notebook.rs`：cell 级签名对齐、source/outputs/execution_count diff；
- cell 级三方合并：优先按 cell `id` 对齐，source 用 diffy 做三方合并，outputs/execution 冲突标记；
- `--base` 对 `.ipynb` 自动走 Notebook 合并；
- `DiffBody::Notebook` + 五个 renderer。

未实现：nbdime 的 Web 渲染；cell 移动已按 id 全局对齐并计入 diff 状态，无 id 时用内容签名估算移动数，但复杂合并冲突启发式仍保持保守策略。

### HDiffPatch（二进制 delta）

阅读文件：`references/HDiffPatch/libHDiffPatch/HDiff/`、`references/HDiffPatch/hpatch/`。

借鉴：二进制补丁/还原的工作流。

DiffEverything 对应：`diff-core/src/patch.rs` 使用 `bsdiff` 生成单文件补丁，并实现纯 Rust HDIFF13 编码器与 zstd/zlib/bzip2/lzma2 解码应用器；同一模块还实现纯 Rust HDIFF19 目录 patch 编码/应用（same-file pair 去重、fadler64 校验、可选 zstd/zlib/lzma2/bzip2 head/hdiff 压缩；应用侧支持官方 zstd/校验目录 patch）；`diff-core/src/folder_patch.rs` 使用自定义 ZIP 容器（manifest + bsdiff/新增文件）实现目录级 patch create/apply；`--patch-format hdiff` 调用官方 hdiffz/hpatchz 处理压缩/校验目录 patch。

未实现：无（HDiffPatch zstd/zlib/lzma2/bzip2 目录 patch 均已有纯 Rust 编码/应用；官方 hpatchz 构建可能缺个别插件，但纯 Rust 应用器可读取）。

### doc_diff_software（OFD / 文档）

阅读：`references/doc-diff-software/README.md`。

DiffEverything 对应：OFD 作为 ZIP 容器读取 `Doc_*/Pages/*/Content.xml`，提取 `TextCode` 文本后走文本 diff。

### gocompare / doc_diff_software

阅读：`references/gocompare/README.md`、`references/doc-diff-software/README.md`。

借鉴：目录/表格/图片/文档的统一入口，以及“先展示差异，再执行操作”的安全工作流。

DiffEverything 对应：本地目录、表格、图片、文档、音频标签/属性、OFD、三方合并、目录同步、WebDAV/HTTP(S)/SFTP/SMB 远程文件、WebDAV/SFTP/SMB 远程目录递归。

未实现：GUI。注册表导出 `.reg` 作为文本 diff；OCR 已实现（图片与扫描 PDF 渲染页）。

## 测试对照

- `cargo test --workspace`：279 个测试；
- `scripts/smoke-test.ps1`：189 个端到端用例；
- `scripts/fetch-testdata.ps1`：从 W3C、Mozilla、python-docx、calamine、plotly、Kubernetes、w3schools、jsonplaceholder、picsum、GNU、Debian、Arch Linux、Fedora、CentOS、Open Diff 下载真实测试文件；
- `testdata/external/MANIFEST.csv`：URL、SHA-256、大小。

第三方项目仅用于研究与功能对照；如果后续需要直接复用其代码，必须按对应许可证处理，不能直接并入 MIT 代码。
