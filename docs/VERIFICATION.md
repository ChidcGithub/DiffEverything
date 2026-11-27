# 验证记录

## 本地环境

- Windows / PowerShell
- Rust 1.97.1（项目 MSRV 1.88）
- 架构：`diff-core` 库 + `diff-everything` CLI

## 最近一次验证

2026-11-27 13:27 CST，`scripts/verify-all.ps1` 依次通过 fmt check、clippy、279 项测试、release 构建和 189 项 smoke。

最终 release 产物：`target/release/diff-everything.exe`，50,950,656 bytes（48.59 MiB），SHA-256 `5B4CA0427B707BB9B384D7FC3ABA4E748FBBD981D3CAA531EB35E4B9EBA48E7D`。



## 测试结果

```text
cargo test --workspace
5 CLI binary unit tests
58 CLI tests
132 diff-core unit tests
6 engine integration tests
58 format integration tests
15 robustness tests
3 folder merge integration tests
2 folder patch integration tests
= 279 tests, all passing
```

```text
cargo test --workspace --release
(279 tests, all passing)
```

```text
cargo clippy --workspace --all-targets -- -D warnings
Finished `dev` profile ... (0 warnings)

cargo fmt --all -- --check
(no diff)
```

```text
cargo build --release -p diff-cli
Finished `release` profile [optimized]
```

```text
powershell -File scripts\smoke-test.ps1 -Exe .\target\release\diff-everything.exe
189 passed, 0 failed
```

smoke test 覆盖：

- 文本 / JSON / YAML / TOML / INI / XML / HTML / Plist（XML + binary）/ Pickle（含未知 global 可读回退）/ Gettext .mo / OpenAPI breaking-change / 跨格式 JSON-YAML、PLIST-JSON、PICKLE-JSON；SQLite 表/行/schema 语义 diff 与物理布局等价判等；结构化数组相似度对齐；ELF/PE/Mach-O 可执行文件语义 diff（含 section 文件偏移、首差异偏移与十六进制片段）；WebAssembly 模块语义 diff；Java class 字段/方法/bytecode 语义 diff；Android APK 二进制 manifest 语义 diff；Git index 条目语义 diff；ICO/CUR 容器语义 diff；IPK 包递归语义 diff；OpenSSH 公钥字段语义 diff（fat Mach-O 按架构切片）；TTF/OTF/WOFF/WOFF2 字体语义 diff；CAB/XAR/AR/TAR 归档递归 diff（含 TAR symlink/hardlink）；外部工具 comparator
- DEB 包（ar → control.tar.gz / data.tar.gz → JSON/INI/text 递归语义 diff）
- TAR.ZST 归档与 TAR.GZ/TAR.ZST 跨压缩等值比较；单文件 ZST 解压后继续 diff
- ISO9660 镜像目录树递归 diff；单文件 BZ2/XZ 解压后继续 diff；CPIO newc / CPIO.GZ 递归 diff；RPM 元数据 + payload 递归 diff；TAR.LZ4 / 单文件 LZ4
- CSV / TSV / DOCX / PPTX / PDF / OFD / XLSX / XLS / ODS / WAV 音频
- 图片 / 二进制 / 目录 / ZIP（含 bzip2/LZMA/zstd 条目）/ 7Z / TAR.GZ / TAR.BZ2 / TAR.XZ / TAR.ZST / TAR.LZ4 / ZST / BZ2 / XZ / LZ4 / CPIO / CPIO.GZ / DEB / RPM / ISO / Notebook
- token diff、完整语法树 diff（内容签名 + 最优二分匹配 + 顺序滑动归一化，大节点 Myers 兜底）、有界语法事件图最短路径 diff（`--mode graph`，非局部精确移动边 + 稳定子树锚点 + 深度无关 token 等价 + LCS 滑块 + 移动检测 `Moved`）、Notebook cell 级 diff/移动/合并、TUI 回退、7Z 归档、unified / Markdown / HTML / PDF 报告
- `--head` / `--last` / `--filter` 内容筛选（按文件/条目限制首尾单元、按路径或内容过滤，支持 glob）、name-only、exclude、summary-only、quiet、ignore whitespace/case
- PDF 文本层页级定位（`# PDF pages` / `# Page N` 标记）、图片热力图 / 并排图 / YIQ 感知容差（`--image-threshold`）/ changed regions 块级定位
- 文本 / Notebook / 目录三方合并干净/冲突、单文件 bsdiff、目录级 patch create/apply、纯 Rust HDIFF13 编码/应用（未压缩/zstd/zlib/bzip2/lzma2）、纯 Rust HDIFF19 目录 patch 编码/应用（fadler64 校验、可选 zstd/zlib/lzma2/bzip2 压缩、可应用官方 zstd/校验目录 patch）、HDiffPatch 兼容 file/folder patch；CAB/XAR/AR 归档递归比较
- 目录同步计划 / 应用 / 备份、会话保存 / 加载
- Tesseract 图片 OCR、PDF 页面 OCR、poppler PDF 视觉报告（HTML 内嵌 base64 PNG + PNG + report.pdf + report-overlay.pdf + report-vector.pdf）
- WebDAV / SFTP / SMB 远程输入（tiny_http、asyncssh、impacket 本地服务器）；WebDAV/SFTP/SMB 目录递归（含空目录）；WebDAV 列表拒绝跨 origin/父目录链接；同名远程文件不会互相覆盖（smoke 通过 release 二进制实测）；SFTP known_hosts 匹配/缺失/被篡改
- 音频标签/属性 diff、OFD 文本 diff

- 下载的真实 PDF / DOCX / XLSX / XLS / ODS / CSV / YAML / XML / JSON / JPEG / TAR.GZ / ZIP / DEB（Debian hello 2.10-2 与 2.10-3，55 个条目递归比较）

## 真实测试文件

`scripts/fetch-testdata.ps1` 下载到 `testdata/external/`，`MANIFEST.csv` 记录 URL、SHA-256、大小。该脚本同时下载可选工具：poppler（pdftoppm）和 portable Tesseract（含 tessdata）。来源包括：

- W3C PDF test files
- Mozilla pdf.js test PDF
- poppler Windows（pdftoppm）
- portable Tesseract 5.5.3 + tessdata_fast eng
- python-docx test documents
- tafia/calamine spreadsheet fixtures
- plotly iris.csv
- Kubernetes website nginx-app.yaml
- w3schools XML examples
- JSONPlaceholder
- picsum images
- GNU hello tar.gz
- Debian hello .deb（两个版本）
- Arch Linux which .pkg.tar.zst（两个版本）
- Fedora hello .rpm（gzip/zstd payload，两个版本）
- CentOS which .rpm（xz payload，两个版本）
- HDiffPatch v5.1.3 Windows binaries（hdiffz/hpatchz）
- Open Diff repository ZIP

部分 403/404 的来源会在脚本中给出 warning 并跳过；当前下载结果可在 `testdata/external/MANIFEST.csv` 查看。

## 参考仓库

`references/` 下已克隆：

```text
diff-pdf          092e6e3
diffoscope        dcfffcb
difftastic        4b834ef
doc-diff-software e3e3f81
gocompare         79539f5
graphtage         1f6688d
HDiffPatch        0696136
nbdime            744c978
open-diff         f42cd1a
uni-diff          8418e97
meld              35d6164
kdiff3            6c4eb7e
diff-so-fancy     cb21bb0
jsdiff            7aac04a
diff-match-patch  62f2e68
pixelmatch        40558aa
openapi-diff      594183e
csvdiff           1007bf3
daff              ad5a02e
diffsitter        a5a6681
```

来源、许可证、具体阅读文件和功能映射见 [REFERENCES.md](REFERENCES.md)。

## 已知边界

- CLI only：没有 GUI；TUI 为终端交互式查看器。
- PDF 默认文本层；视觉 diff 需要可选 poppler，OCR 需要可选 Tesseract（脚本会下载 portable 版本）。
- WebDAV / HTTP(S) / SFTP / SMB 文件输入已支持；WebDAV / SFTP / SMB 支持目录递归。
- `--sync` 仅支持本地目录；远程输入会在下载前被拒绝，避免只修改临时副本。
- `--external-tool` 接收解析后的本地路径；远程输入会先下载到临时目录，外部程序看到临时路径。
- ZIP LZMA（method 14）通过重建 LZMA-Alone 头解码；不支持/损坏的条目回退二进制。
- SMB 集成测试由 `scripts/smb-test-server.py`（Python impacket）提供本地 SMB2 共享。
- SFTP 可选 `--sftp-known-hosts`：主机密钥不匹配或缺失时拒绝连接；默认仍不校验（便于临时测试服务器）。
- 旧版 `.doc`/`.ppt` 回退二进制；`.xls`/`.ods` 由 calamine 支持。
- 加密 ZIP、未知压缩算法回退二进制并 warning。
- 目录同步只处理文件，不跟随符号链接；目录/文件类型冲突会报错。
