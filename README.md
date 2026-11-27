# DiffEverything

用 Rust 编写的 **CLI 统一文件对比工具**。目标不是替代 `diff`，而是把不同类型文件先归一化成可比较的中间表示，再用同一棵差异树输出终端、JSON、Markdown、HTML 或 unified diff。

当前版本：**v0.2.0（仅 CLI）**。参考并融合了 diffoscope、Graphtage、difftastic、Open Diff、uni-diff、diff-pdf、nbdime、HDiffPatch 等项目的核心 diff 思路，详见 [docs/REFERENCES.md](docs/REFERENCES.md)。

## 功能总览

| 类别 | 格式 | 已实现能力 |
|---|---|---|
| 文本 / 代码 | 任意文本、UTF-8 / UTF-16 / GBK、.reg 注册表导出等 | 行级 diff、上下文、行内 word-level 高亮、忽略空白 / 大小写 |
| 语法感知 | Rust、Python、JS/JSX、TS/TSX、C、C++、Go、Java、Bash、Ruby、C#、Swift、Lua | tree-sitter token diff + 完整语法树 diff（`--mode tree`）+ 有界语法事件图最短路径 diff（`--mode graph`，含非局部移动边），忽略格式变化、可忽略注释 |
| 结构化数据 | JSON / JSON5 / YAML / TOML / INI / XML / HTML / Plist（XML + binary）/ Pickle / Gettext (.mo) / OpenAPI | 语义树 diff，路径级 added / removed / changed；数组相似度对齐；OpenAPI 自动标记 breaking change |
| 数据库 | SQLite 3 `.db` / `.sqlite` | 表、列、行、索引/触发器 schema 语义 diff；不同 page size/物理布局的等价数据库判等 |
| 跨格式 | JSON ↔ YAML ↔ TOML ↔ INI ↔ XML ↔ HTML ↔ Plist ↔ Pickle | 先解析为统一树再比较，例如 `YAML+JSON`、`PLIST+JSON`、`PICKLE+JSON` |
| 表格 | CSV / TSV | 表头、行、单元格级 diff，支持 `--key` 按列对齐 |
| 电子表格 | XLSX / XLS / ODS | 工作表、单元格级 diff（calamine） |
| 文档 | PDF / OFD / DOCX / PPTX | PDF 文本层 + 页数 + 页面级文本块定位 + 页面渲染视觉 diff（HTML/PNG/PDF 报告）；OFD 页面文本；DOCX 正文/页眉页脚；PPTX 幻灯片/备注 |
| 图片 / OCR | PNG / JPEG / GIF / WebP / BMP / TIFF / ICO / CUR | 像素差异比例、YIQ 感知容差（`--image-threshold`）、最大通道差、包围盒、changed regions 块级定位、热力图、并排图；Tesseract OCR 文本 diff；PDF 扫描页 OCR |
| 音频 | MP3 / FLAC / OGG / M4A / WAV / AAC / OPUS / WMA | 时长、码率、采样率、声道、标签元数据 diff（lofty） |
| 二进制 / 补丁 | 任意二进制 / 目录 | Hex 行 diff、精确差异字节数、首个差异偏移、单文件 bsdiff；目录级 ZIP patch；纯 Rust 编码/应用 HDIFF13（未压缩/zstd/zlib/bzip2/lzma2 buffer）；纯 Rust HDIFF19 目录 patch 编码/应用（fadler64 校验、same-file 去重、可选 zstd/zlib/lzma2/bzip2 head/hdiff 压缩；可应用官方 zstd/校验目录 patch，hpatchz 兼容） |
| OpenSSH public key | `.pub`（`ssh-*` / `ecdsa-*` / `sk-*`） | 解析 base64 wire blob：key type、RSA e/n、DSA p/q/g/y、ECDSA curve/point、Ed25519 key、bits、comment |
| IPK | `.ipk` | OpenWrt 包：外层 gzip → tar → control.tar.gz/data.tar.gz 递归语义 diff |
| ICO / CUR | `.ico` / `.cur` | 容器目录、尺寸、色深、planes、PNG/BMP 内嵌格式与 payload digest 的语义 diff |
| Git index | `.git/index`（`DIRC`） | 版本 / 路径 / mode / stage / blob SHA / stat 元数据的语义 diff；按路径排序，忽略 padding 与扩展布局 |
| Android APK | `.apk` / binary `AndroidManifest.xml` | ZIP 递归 + Android 二进制 XML（string pool/chunk）解析；元素/命名空间/属性/typed value 语义 diff；`--detail full -y` 自动解包并用内置 DEX 反编译器比较 classes.dex 的 Java 代码，并对内嵌 .so 用内嵌反汇编器做代码 diff；损坏 manifest 回退二进制 |
| Java class | `.class` | 版本 / 类名 / 父类 / 接口 / 字段 / 方法 / access flags / 方法 bytecode digest 的语义 diff；损坏 class 回退二进制 |
| WebAssembly | `.wasm` | types / imports / exports / functions / code body digest / custom sections 的语义 diff；损坏模块回退二进制 |
| 可执行文件 | ELF / PE / Mach-O / COFF | 架构、入口、节 digest、文件偏移、符号、导入库/导入符号语义 diff；fat Mach-O 按架构切片递归；损坏文件回退二进制；`--detail full` 可对 ELF/PE/Mach-O 可执行文件或 .so 调用反汇编工具做地址归一化代码 diff（需 y/n 授权，`-y` 自动同意） |
| 字体 | TTF / OTF / TTC / WOFF / WOFF2 | 家族、样式、字重/宽度、单位、升降部、glyph 数量、ASCII advance、粗体/斜体等语义 diff；WOFF/WOFF2 解包为 SFNT 后比较；损坏字体回退二进制 |
| 目录 | 递归目录 | 新增 / 删除 / 修改 / 相同，`--exclude` glob |
| 归档 / 镜像 / 包 | ZIP / ZIP(bzip2/lzma/zstd) / 7Z / TAR / TAR.GZ / TAR.BZ2 / TAR.XZ / TAR.ZST / TAR.LZ4 / CPIO / CPIO.GZ / CAB / XAR / AR / IPK / GZ / ZST / BZ2 / XZ / LZ4 / DEB / RPM / ISO | 递归解包，按条目类型继续分派；DEB（ar）与 RPM（header+payload）解析后继续比较；ISO9660 目录树递归；CPIO newc；CAB（MSZIP/LZX）、XAR（zlib/bzip2/xz/lzma）、AR（GNU/BSD 长名、symbol table）与 TAR symlink/hardlink 递归；单文件 GZ/ZST/BZ2/XZ/LZ4 解压后继续比较 |
| Notebook | `.ipynb` | cell 级 diff、按 cell id 全局对齐（移动不误报增删）、输出/执行次数变化、cell 级三方合并 |
| 合并 | 文本 / Notebook / 目录 | 三方合并；目录合并输出到新目录，文本冲突写入标准冲突标记，删除/修改冲突保留修改侧并报告 |
| 报告 | — | ANSI、交互式 TUI、JSON、Markdown、自包含 HTML、unified diff |
| 工作流 | — | 退出码、`--name-only`、`--quiet`、`--summary-only`、大文件流式兜底、目录同步、会话保存/加载、外部工具委托比较 |
| 远程输入 | WebDAV / HTTP(S) / SFTP / SMB | 文件或目录下载到临时目录后像本地一样比较；WebDAV/SFTP/SMB 目录递归；支持 URL 或 `--remote-user/--remote-password` |

## 快速开始

需要 Rust 1.88+。

```powershell
cargo build --release -p diff-cli
.\target\release\diff-everything.exe old new
```

常用示例：

```powershell
# 文本（行内高亮在终端颜色模式下显示）
.\target\release\diff-everything.exe fixtures\text\old.txt fixtures\text\new.txt

# JSON / YAML / TOML / INI / XML 语义 diff
.\target\release\diff-everything.exe fixtures\structured\old.yaml fixtures\structured\new.yaml

# 跨格式：YAML vs JSON
.\target\release\diff-everything.exe fixtures\structured\cross.yaml fixtures\structured\cross.json

# Plist（XML 与 binary 均可解析；也可与 JSON 跨格式比较）
.\target\release\diff-everything.exe fixtures\plist\old.plist fixtures\plist\new.plist
.\target\release\diff-everything.exe fixtures\plist\old.plist fixtures\plist\old-binary.plist

# Pickle（Python protocol 2-5；可与 JSON 跨格式比较）
.\target\release\diff-everything.exe fixtures\pickle\old.pkl fixtures\pickle\new.pkl
.\target\release\diff-everything.exe fixtures\pickle\old.pkl fixtures\pickle\old.json

# CSV 按 id 列对齐
.\target\release\diff-everything.exe fixtures\table\old.csv fixtures\table\new.csv --key id

# 代码语法 token diff / 完整语法树 diff
.\target\release\diff-everything.exe fixtures\code\old.rs fixtures\code\new.rs --mode token --ignore-comments
.\target\release\diff-everything.exe fixtures\code\old.rs fixtures\code\new.rs --mode tree --ignore-comments

# 图片 OCR（需要 tesseract；脚本会下载 portable 版本）
.\target\release\diff-everything.exe fixtures\ocr\old.png fixtures\ocr\new.png --ocr

# PDF 页面渲染视觉 diff（需要 pdftoppm；脚本会下载 poppler）
# 输出 report.html、report.pdf、report-overlay.pdf、report-vector.pdf、heatmaps/、side-by-side/、overlays/
.\target\release\diff-everything.exe fixtures\pdf\old.pdf fixtures\pdf\new.pdf --pdf-visual --pdf-output-dir pdf-report

# 扫描版 PDF OCR（需要 tesseract + pdftoppm）
.\target\release\diff-everything.exe fixtures\pdf\old.pdf fixtures\pdf\new.pdf --ocr --pdf-max-pages 5

# 目录同步（默认只打印计划，--sync-apply 才修改文件）
.\target\release\diff-everything.exe left-folder right-folder --sync mirror-left-to-right
.\target\release\diff-everything.exe left-folder right-folder --sync mirror-left-to-right --sync-apply --backup-dir backup

# 会话保存/加载
.\target\release\diff-everything.exe a.json b.json --save-session session.json
.\target\release\diff-everything.exe --load-session session.json

# WebDAV / HTTP(S) / SFTP / SMB 远程输入（目录会自动递归下载）
.\target\release\diff-everything.exe webdav://user:pass@host/old.txt https://host/new.txt
.\target\release\diff-everything.exe webdav://user:pass@host/left/ webdav://user:pass@host/right/
.\target\release\diff-everything.exe sftp://user:pass@host:22/old.txt sftp://user:pass@host:22/new.txt
.\target\release\diff-everything.exe sftp://host/old.txt sftp://host/new.txt --remote-user user --remote-password pass
.\target\release\diff-everything.exe sftp://host/old.txt sftp://host/new.txt --sftp-known-hosts $HOME\.ssh\known_hosts
.\target\release\diff-everything.exe smb://user:pass@host:445/share/old.txt smb://host/share/new.txt --remote-user user --remote-password pass

# APK：完整模式自动解包 ZIP，并用内置 DEX 反编译器比较 classes.dex 的 Java 代码
.\target\release\diff-everything.exe old.apk new.apk --detail full -y

# 完整模式：可执行文件/.so 会询问是否允许反编译（-y 自动同意）
.\target\release\diff-everything.exe a.so b.so --detail full
.\target\release\diff-everything.exe a.exe b.exe --detail full -y
.\target\release\diff-everything.exe a.elf b.elf --detail full -y

# 简略模式 / 全量模式
.\target\release\diff-everything.exe old.txt new.txt --detail concise
.\target\release\diff-everything.exe old.txt new.txt --detail full

# 精准筛选输出：每个文件/条目只保留前/后 N 个内容单元
.\target\release\diff-everything.exe old.txt new.txt --head 20
.\target\release\diff-everything.exe old.txt new.txt --last 20

# 按路径/标签或内容过滤，可重复；支持 glob
.\target\release\diff-everything.exe left/ right/ --filter "*.json"
.\target\release\diff-everything.exe old.txt new.txt --filter "needle" --context 2

# 忽略空白和大小写
.\target\release\diff-everything.exe old.txt new.txt --ignore-whitespace --ignore-case

# 文档 / 表格
.\target\release\diff-everything.exe fixtures\docs\old.docx fixtures\docs\new.docx
.\target\release\diff-everything.exe fixtures\workbook\old.xlsx fixtures\workbook\new.xlsx

# SQLite / ELF / 字体语义比较
.\target\release\diff-everything.exe fixtures\sqlite\old.db fixtures\sqlite\new.db
.\target\release\diff-everything.exe fixtures\executable\hello-old fixtures\executable\hello-new
.\target\release\diff-everything.exe fixtures\font\regular.ttf fixtures\font\bold.ttf

# 图片：统计 + 热力图 + 并排图
.\target\release\diff-everything.exe fixtures\images\old.png fixtures\images\new.png `
  --image-heatmap heatmap.png --image-side-by-side side-by-side.png

# 目录 / ZIP / TAR.GZ
.\target\release\diff-everything.exe fixtures\dir\left fixtures\dir\right --exclude "*.log"
.\target\release\diff-everything.exe fixtures\zip\old.zip fixtures\zip\new.zip
.\target\release\diff-everything.exe fixtures\archives\old.tar.gz fixtures\archives\new.tar.gz
.\target\release\diff-everything.exe fixtures\deb\old.deb fixtures\deb\new.deb
.\target\release\diff-everything.exe fixtures\archive-zst\old.tar.zst fixtures\archive-zst\new.tar.zst
.\target\release\diff-everything.exe fixtures\archive-zst\old.log.zst fixtures\archive-zst\new.log.zst
.\target\release\diff-everything.exe fixtures\iso\old.iso fixtures\iso\new.iso
.\target\release\diff-everything.exe fixtures\compression\old.txt.xz fixtures\compression\new.txt.xz
.\target\release\diff-everything.exe fixtures\cpio\old.cpio fixtures\cpio\new.cpio
.\target\release\diff-everything.exe testdata\external\hello-2.12.1-2.fc39.x86_64.rpm testdata\external\hello-2.12.1-4.fc40.x86_64.rpm
.\target\release\diff-everything.exe fixtures\lz4\old.tar.lz4 fixtures\lz4\new.tar.lz4

# 三方合并（0=干净，1=冲突；.ipynb 自动走 cell 级合并）
.\target\release\diff-everything.exe left.txt right.txt --base base.txt --merge-output merged.txt
.\target\release\diff-everything.exe ours.ipynb theirs.ipynb --base base.ipynb --merge-output merged.ipynb

# 目录三方合并（输出目录必须为空或不存在）
.\target\release\diff-everything.exe ours-folder theirs-folder --base base-folder --merge-output merged-folder

# 交互式 TUI（管道/重定向时自动回退为 ANSI）
.\target\release\diff-everything.exe fixtures\text\old.txt fixtures\text\new.txt --format tui

# 生成二进制补丁
.\target\release\diff-everything.exe old.bin new.bin --patch-output change.patch

# 目录级补丁：生成后应用到新目录（不修改源目录）
.\target\release\diff-everything.exe old-folder new-folder --patch-output folder.patch
.\target\release\diff-everything.exe old-folder --apply-patch folder.patch --merge-output patched-folder

# 外部工具 comparator（LEFT/RIGHT 会追加到参数末尾）
.\target\release\diff-everything.exe old.bin new.bin --external-tool my-diff-tool

# 单文件 HDIFF13 与目录 HDIFF19 均可纯 Rust 编码/应用；压缩/校验目录 patch 需要 hdiffz
.\target\release\diff-everything.exe old.bin new.bin --patch-format hdiff --hdiffz hdiffz.exe --patch-output change.hdiff
.\target\release\diff-everything.exe old.bin --patch-format hdiff --hpatchz hpatchz.exe --apply-patch change.hdiff --merge-output new.bin

# 报告
.\target\release\diff-everything.exe fixtures\dir\left fixtures\dir\right --format html --output report.html
.\target\release\diff-everything.exe fixtures\dir\left fixtures\dir\right --format json --output report.json
```

## CLI 参数

| 参数 | 说明 |
|---|---|
| **输入与比较** | |
| `<LEFT>` `<RIGHT>` | 两个文件 / 目录；也可用 WebDAV / HTTP(S) / SFTP / SMB URL |
| `--format <ansi\|tui\|json\|markdown\|html\|unified>` | 输出格式 |
| `--mode <text\|token\|tree\|graph>` | 行文本 / tree-sitter token / 完整语法树 / 语法事件图 |
| `--context <N>` | 文本上下文行数（默认 3） |
| `--ignore-comments` | 忽略注释；支持的源码语言自动使用 token 模式 |
| `--ignore-whitespace` / `--ignore-case` | 文本比较忽略空白 / 大小写 |
| `--key <COLUMN>` | CSV/TSV 行对齐键 |
| `--exclude <GLOB>` | 目录比较排除路径，可重复 |
| `--image-threshold <0..1>` | 图片 YIQ 感知容差；越大越忽略轻微颜色差异，默认 0 |
| **输出与筛选** | |
| `-o, --output <FILE>` | 写入文件 |
| `--detail <concise\|full>` | 简略模式删除相等上下文/条目并限制详情；完整模式保留完整报告并支持反编译，默认 concise |
| `--head <N>` / `--last <N>` | 每个文件/条目只显示前/后 N 个内容单元 |
| `--filter <PATTERN>` | 按路径/标签或内容匹配；可重复，支持 glob；文本保留匹配行及 `--context` 上下文 |
| `--name-only` / `--quiet` / `--summary-only` | 只输出变更路径 / 无输出 / 只输出摘要和变更路径 |
| `--no-color` | 关闭 ANSI 颜色 |
| **资源与递归限制** | |
| `--max-rows <N>` / `--max-table-rows <N>` / `--max-workbook-cells <N>` | 明细、表格行、工作簿单元上限 |
| `--max-file-bytes <N>` / `--max-entries <N>` | 单文件字节数 / 目录与归档条目上限 |
| `--max-depth <N>` | 目录/归档递归深度上限，默认 32（1..1024） |
| **合并与同步** | |
| `--base <FILE\|DIR>` | 三方合并 base（目录 base 自动进入目录三方合并） |
| `--merge-output <FILE\|DIR>` | 合并结果输出；目录合并必须是空目录或新目录 |
| `--sync <MODE>` / `--sync-apply` / `--backup-dir <DIR>` | 目录同步计划 / 应用 / 覆盖删除前备份 |
| **补丁** | |
| `--patch-output <FILE>` | 生成单文件 bsdiff 或目录级 patch |
| `--apply-patch <FILE>` | 将目录级 patch 或 HDiffPatch 应用到 LEFT，结果写入 `--merge-output` |
| `--patch-format <bsdiff\|hdiff\|hdiff-rust>` | 补丁格式；`hdiff` 调用 hdiffz/hpatchz，`hdiff-rust` 使用纯 Rust HDIFF13/HDIFF19 |
| `--patch-compress <none\|zstd\|zlib\|lzma2\|bzip2>` | `hdiff-rust` 文件/目录补丁压缩方式 |
| `--hdiffz <FILE>` / `--hpatchz <FILE>` | HDiffPatch 工具路径；不传时使用 PATH |
| **会话与远程** | |
| `--save-session <FILE>` / `--load-session <FILE>` | 保存 / 加载比较会话 |
| `--remote-user` / `--remote-password` | WebDAV / SFTP / SMB 认证 |
| `--sftp-known-hosts <FILE>` | 按 OpenSSH known_hosts 校验 SFTP 主机密钥 |
| **文档 / 图片 / OCR** | |
| `--image-heatmap <FILE>` / `--image-side-by-side <FILE>` | 图片差异热力图 / 左右并排图 |
| `--pdf-visual` / `--pdftoppm <FILE>` / `--pdf-dpi <N>` / `--pdf-max-pages <N>` / `--pdf-output-dir <DIR>` | PDF 页面渲染视觉 diff |
| `--ocr` / `--tesseract <FILE>` / `--tessdata <DIR>` / `--ocr-language <LANG>` | 图片/PDF OCR 与工具配置 |
| **反编译与外部工具** | |
| `-y, --yes` | 完整模式下自动同意反编译（跳过 y/n 询问） |
| `--objdump <FILE>` | 可选：改用外部 llvm-objdump/objdump；默认使用内嵌纯 Rust 反汇编器 |
| `--external-tool <PROGRAM>` / `--external-arg <ARG>` | 委托外部程序比较，LEFT/RIGHT 追加到参数末尾 |

内容筛选只影响展示，不改变比较状态、退出码或摘要统计；`--head`/`--last` 会标记 `(truncated)`。

退出码：`0` 相同，`1` 有差异 / 合并冲突，`2` 运行错误。

## 架构

```text
CLI
 │
 ▼
diff-core
 │
 ├─ detect      扩展名 + magic bytes + 文本启发式
 ├─ read        内存读取 / 大文件流式检查
 ├─ normalize   text / JSON tree / pixels / table / workbook / document text / bytes
 ├─ compare     格式专用算法
 ├─ model       DiffReport → DiffNode tree → DiffBody
 └─ render      ANSI / JSON / Markdown / HTML / unified
```

核心模型是 `DiffNode` 树：目录和归档是容器节点，文本 / 结构化 / 表格 / 工作簿 / 图片 / 二进制是叶子节点。渲染器只消费模型，不读文件，方便后续加 TUI 或 GUI。

模块与新增格式步骤见 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)。

## 参考项目与功能融合

| 项目 | 借鉴的核心能力 | DiffEverything 落地 |
|---|---|---|
| [diffoscope](https://diffoscope.org) | 递归解包、比较器/呈现器分层、HTML/JSON 报告 | ZIP/TAR/CAB/XAR/AR 递归、PDF/Office 文本、SQLite 语义、ELF/PE 可执行文件语义、字体语义、多格式报告、资源限制 |
| [Graphtage](https://github.com/trailofbits/graphtage) | JSON/YAML/TOML/INI/XML/CSV 语义树 diff、跨格式比较 | 结构化语义 diff、数组相似度对齐（插入/删除不级联误报）、`YAML+JSON` 跨格式、CSV 表格 diff |
| [difftastic](https://github.com/Wilfred/difftastic) | tree-sitter 解析、忽略注释、语法树比较 | 13 种语言 token diff + 完整语法树 diff（内容签名 + 最优二分匹配（匈牙利）+ 顺序滑动归一化，大节点 Myers 兜底）、`--ignore-comments` |
| [Open Diff](https://github.com/kygo8/open-diff) | 文本/表格/图片/Hex/二进制/目录、三方合并、目录同步、会话、HTML 报告 | 全部 CLI 对应能力、bsdiff、热力图/并排图、同步/会话 |
| [uni-diff](https://github.com/neardws/uni-diff) | PDF/DOCX/XLSX/PPTX、OCR、TUI、多种输出、block diff | 文档格式、OCR、TUI、ANSI/JSON/Markdown/HTML、块级 hunk |
| [diff-pdf](https://github.com/vslavik/diff-pdf) | PDF 视觉比较 | 文本层 + 页数 + poppler 页面渲染视觉 diff/热力图/报告 |
| [nbdime](https://github.com/jupyter/nbdime) | Notebook diff/merge | cell 级 diff、输出/执行次数、cell 级三方合并 |
| [HDiffPatch](https://github.com/sisong/HDiffPatch) | 二进制 / 目录 diff/patch | 单文件 bsdiff、目录级 ZIP patch、纯 Rust HDIFF13 编码/应用（未压缩/zstd/zlib/bzip2/lzma2）、纯 Rust HDIFF19 目录 patch 编码/应用（fadler64 校验、same-file 去重、可选 zstd/zlib/lzma2/bzip2 压缩，可应用官方 zstd/校验目录 patch）；`--patch-format hdiff` 调用 hdiffz/hpatchz |
| [gocompare](https://github.com/jhzhang09/gocompare) | 目录/表格/图片/三方合并/同步 | 目录、表格、图片、文本/Notebook/目录三方合并、同步；额外实现 WebDAV/SFTP/SMB 远程输入 |

另外还克隆了 Meld、KDiff3、diff-so-fancy、jsdiff、diff-match-patch、pixelmatch、openapi-diff、csvdiff、daff、diffsitter 等 10 个相关项目，共 20 个参考仓库。具体文件、许可证和“借鉴了什么”见 [docs/REFERENCES.md](docs/REFERENCES.md)；逐项能力、实现文件与测试证据见 [docs/FEATURE-MATRIX.md](docs/FEATURE-MATRIX.md)。

## 测试与验证

```powershell
# 279 个 Rust 测试（单元 + 集成 + CLI）
cargo test --workspace

# 严格 lint
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# 外部真实文件下载（PDF/DOCX/XLSX/CSV/YAML/XML/图片/归档）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\fetch-testdata.ps1

# 重新生成 SQLite / ELF 夹具（ELF 来自真实 Debian hello 包）
python scripts\make-fixtures.py

# 189 项端到端 smoke test
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\smoke-test.ps1

# 一键运行 fmt + clippy + 全量测试 + release 构建 + smoke
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\verify-all.ps1
```

完整记录见 [docs/VERIFICATION.md](docs/VERIFICATION.md)，参考项目逐项对照见 [docs/FEATURE-MATRIX.md](docs/FEATURE-MATRIX.md)。

测试覆盖：

- 每个格式的叶子节点类型、差异数量、关键路径/单元格/行号
- 无效 JSON/ZIP/PDF/DEB/RPM/ISO/CPIO 的回退
- UTF-16 文本、空文件、大文件流式路径
- ZIP 条目限制、目录排除
- CLI 退出码、JSON 可解析、HTML 闭合、Markdown/Unified 结构
- 图片热力图 / 并排图、文本 / Notebook / 目录三方合并干净/冲突、单文件 bsdiff、目录级 patch create/apply、纯 Rust HDIFF13 编码/应用（未压缩/zstd/zlib/bzip2/lzma2）、HDiffPatch 兼容补丁
- 语法树 diff、目录同步计划/应用/备份、会话保存/加载
- Notebook cell 级 diff/移动/三方合并、TUI 回退
- WebDAV / SFTP / SMB 本地测试服务器、远程文件与目录递归（含空目录）输入、SFTP known_hosts 匹配/缺失/不匹配
- OFD 文本、音频时长/码率/采样率/声道/标签
- Tesseract OCR（图片 + PDF 渲染页）、poppler PDF 视觉报告（HTML/PDF/overlay PDF）
- Plist XML / binary / 跨格式（PLIST+JSON）语义 diff
- OpenAPI breaking-change 标记（删除 operation/path、必需参数、response 等）
- 外部工具委托 comparator（stdout/stderr、0/1/其他退出码映射）
- Pickle protocol 4/5 / 跨协议 / 跨格式（PICKLE+JSON）语义 diff；未知 global 提取可读字符串
- DEB ar 容器与内嵌 control/data tar 递归 diff
- TAR.ZST（含跨压缩格式 TAR.GZ vs TAR.ZST 等值）与单文件 ZST 解压比较
- ISO9660 镜像目录树递归 diff
- 单文件 BZ2 / XZ 解压后继续 diff
- CPIO newc（含 CPIO.GZ 链式解压）目录条目递归 diff
- RPM（元数据 + gzip/xz/zstd payload 文件）递归 diff
- TAR.LZ4 与单文件 LZ4 解压比较
- ZIP 的 bzip2/LZMA/zstd 条目压缩
- 下载的真实 PDF/DOCX/XLSX/XLS/ODS/CSV/YAML/XML/JSON/JPEG/TAR.GZ/ZIP

## 限制（CLI 范围）

- 不做 GUI；TUI 为交互式终端查看器，管道/重定向时自动回退 ANSI。
- PDF 默认走文本层；`--pdf-visual` 页面渲染需要 poppler `pdftoppm`，`--ocr` 需要 Tesseract。脚本可下载 portable 版本。
- PDF 视觉输出：`report.pdf`（并排+热力图）、`report-overlay.pdf`（栅格红色叠加）、`report-vector.pdf`（在原 PDF 页面内容流上追加红色矢量矩形），以及 HTML/PNG；`report.html` 内嵌 base64 PNG，可单独分发。
- WebDAV / HTTP(S) / SFTP / SMB 均支持文件输入；WebDAV / SFTP / SMB 支持目录递归。
- RPM 支持 gzip/xz/zstd payload；ZIP 支持 bzip2/LZMA（method 14）/zstd 条目。
- `--sync` 仅作用于本地目录；远程输入会拒绝同步，避免误改临时副本。
- `--external-tool` 接收解析后的本地路径；远程输入会先下载到临时目录，外部程序看到的是临时路径。
- SFTP 默认不校验主机指纹；生产环境应传入 `--sftp-known-hosts <FILE>`，主机密钥缺失或不匹配时会拒绝连接。
- 旧版 `.doc` / `.ppt` 回退二进制；`.xls` / `.ods` 由 calamine 支持。
- 默认详细 diff 上限 64 MiB；目录 10,000 项；归档 2,000 项；可通过参数调整。
- 加密 ZIP / 不支持压缩算法的归档会回退二进制并给出 warning。

## License

MIT。参考项目仅用于学习算法与功能设计；diffoscope（GPL-3）、diff-pdf（GPL-2）、Graphtage（LGPL-3）等代码未复制进本项目。
