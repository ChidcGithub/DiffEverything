# 功能融合矩阵

本文件把 20 个参考项目的核心能力逐一映射到 DiffEverything 的实现文件、测试和 smoke 用例，作为“查漏补缺”的清单。参考仓库版本与许可证见 [REFERENCES.md](REFERENCES.md)。

## 参考项目映射

| 参考项目 | 借鉴能力 | DiffEverything 实现 | 验证证据 | 状态 |
|---|---|---|---|---|
| CLI | APK 自动解包 + DEX 反编译 | `crates/diff-core/src/diff/dex.rs`、`engine.rs` | `decompiles_dex_to_java`、`compares_apk_dex_decompiled_code`、`full_detail_decompiles_apk_dex_with_yes`、`full_detail_decompiles_apk_native_libs`、smoke `apk dex decompile`/`apk native decompile` | 已完成（ZIP 解包 → classes.dex → droidsaw-dex Java 源码 diff；内嵌 .so → 内嵌 yaxpeax 反汇编代码 diff） |
| CLI | 可执行文件/共享库反编译对比 | `crates/diff-core/src/disassembly.rs`、`crates/diff-cli/src/main.rs` | `strips_instruction_and_label_addresses`、`full_detail_decompiles_executables_with_yes`、`full_detail_without_yes_skips_decompile_when_noninteractive`、smoke `decompile full -y` | 已完成（完整模式；默认内嵌 yaxpeax ARM/AArch64/x86/x64 反汇编，`--objdump` 可选覆盖；运行前 y/n 授权，`-y` 跳过；地址归一化） |
| CLI | 递归深度上限 | `crates/diff-cli/src/main.rs`、`engine.rs` | `max_depth_limits_nested_archives_and_validates_zero` | 已完成（`--max-depth 1..1024`，默认 32） |
| CLI | 简略/全量 diff 模式 | `crates/diff-core/src/concise.rs`、`crates/diff-cli/src/main.rs` | `concise_text_drops_equal_context`、`concise_archive_drops_equal_entries`、`concise_detail_suppresses_equal_context`、smoke `concise text` | 已完成（`--detail concise` 删除相等上下文/条目并限制详情；`full` 保持完整） |
| diffoscope | 内容筛选/报告裁剪 | `crates/diff-core/src/filter.rs`、`crates/diff-cli/src/main.rs` | `head_last_and_filter_select_text_content`、`filter_selects_paths_in_directories`、`filter_limits_json_hunks_and_head_rejects_zero`、smoke `head/last/filter` | 已完成（`--head`/`--last`/`--filter`，所有渲染器共享） |
| diffoscope | 按类型分派 comparator | `crates/diff-core/src/engine.rs`、`detect.rs` | `cargo test --workspace` | 已完成 |
| diffoscope | ZIP/7Z/TAR/GZ/BZ2/XZ/ZST/LZ4/CPIO 递归 | `engine.rs` 的 archive 分支 | `compares_tar_gz_and_zip_archives`、`compares_tar_symlinks_and_hardlinks`、`compares_zip_bzip2_and_zstd_entries`、`compares_tar_zst_archives`、`compares_lz4_archives_and_single_files`、`compares_cpio_archives`、`compares_deb_packages_recursively` | 已完成（ZIP bzip2/LZMA/zstd；TAR symlink/hardlink） |
| diffoscope | CAB 递归 | `engine.rs` 的 `compare_cab_bytes`/`read_cab_entries` | `compares_cab_archives`、`invalid_cab_falls_back_to_binary`、smoke `cab archive` | 已完成（MSZIP/LZX） |
| diffoscope | XAR 递归 | `engine.rs` 的 `compare_xar_bytes`/`read_xar_entries` | `compares_xar_archives`、`compares_xar_compression_codecs`、`compares_xar_symlinks`、smoke `xar archive`/`xar equal`/`xar codecs`/`xar symlink` | 已完成（TOC zlib + zlib/bzip2/xz/lzma/raw 条目 + symlink，libarchive 交叉验证） |
| diffoscope | AR 静态库 | `engine.rs` 的 `compare_ar_bytes`/`read_ar_entries` | `compares_ar_archives`、`compares_bsd_ar_archives`、smoke `ar archive`/`bsd ar archive` | 已完成（GNU 长名/符号表、BSD `#1/` 长名，条目递归进 ELF） |
| diffoscope | DEB / RPM / ISO 专用 comparator | `compare_deb_bytes`、`compare_rpm_bytes`、`compare_iso_bytes` | `compares_deb_packages_recursively`、`compares_real_rpm_packages`、`compares_real_xz_payload_rpm_packages`、`compares_iso_images`；smoke `deb package`、`external rpm diff`、`external rpm xz diff`、`iso image` | 已完成（gzip/xz/zstd payload） |
| diffoscope | SQLite 数据库 | `diff/sqlite.rs`、`engine.rs` | `extracts_schema_columns_and_rows`、`rejects_non_database_bytes`、`compares_sqlite_databases_semantically`、`invalid_sqlite_falls_back_to_binary`、smoke `sqlite semantic`/`sqlite page layout equal` | 已完成（表/列/行/索引与触发器 schema，物理布局无关） |
| diffoscope | ELF / PE / Mach-O 可执行文件 | `diff/executable.rs`、`engine.rs` | `extracts_elf_sections_symbols_and_libraries`、`rejects_non_executable_bytes`、`compares_elf_executables_semantically`、`compares_pe_executables_semantically`、`compares_pe_fixtures_semantically`、`compares_macho_executables_semantically`、`invalid_executable_falls_back_to_binary`、smoke `elf semantic`/`elf byte offsets`/`pe semantic`/`pe fixture semantic`/`macho fat semantic` | 已完成（节 digest、文件偏移、首差异绝对/相对偏移与十六进制片段、符号、导入库/符号、架构/入口；Mach-O fat 按架构切片解析；COFF 同一解析器） |
| diffoscope | WebAssembly | `diff/wasm.rs`、`structured.rs` | `parses_exports_functions_and_code`、`rejects_truncated_binary`、`compares_wasm_modules`、smoke `wasm semantic`/`wasm equal` | 已完成（types/imports/exports/functions/code digest/custom sections；Node `WebAssembly.validate` 验证） |
| diffoscope | Java class | `diff/java_class.rs`、`structured.rs` | `parses_class_methods_and_code`、`rejects_truncated_class`、`compares_java_class_files`、`compares_jar_java_class_semantically`、smoke `java class semantic`/`java class equal`/`jar java class` | 已完成（版本/类/父类/接口/字段/方法/bytecode digest；javap 交叉验证） |
| diffoscope | Android APK / binary XML | `diff/android_xml.rs`、`structured.rs` | `parses_manifest_attributes_and_tree`、`rejects_truncated_manifest`、`compares_apk_manifests_semantically`、smoke `apk manifest`/`apk manifest equal` | 已完成（ZIP 递归 + string pool/chunk/typed value；androguard 交叉验证） |
| diffoscope | Git index | `diff/git_index.rs`、`structured.rs` | `parses_entries_and_modes`、`parses_multiple_entries`、`rejects_truncated_index`、`compares_git_indexes`、smoke `git index`/`git index equal` | 已完成（DIRC v2/v3、路径/mode/stage/SHA/stat；按路径排序） |
| diffoscope | ICO / CUR | `diff/ico.rs`、`structured.rs` | `parses_icon_directory`、`rejects_truncated_ico`、`compares_ico_images`、smoke `ico semantic`/`ico equal` | 已完成（目录/尺寸/色深/PNG-BMP 格式/payload digest） |
| diffoscope | IPK 包 | `engine.rs` 的 gzip/tar 递归分支 | `compares_ipk_packages_recursively`、smoke `ipk package`/`ipk package equal` | 已完成（OpenWrt gzip → tar → control/data tar.gz 递归） |
| diffoscope | OpenSSH public key | `diff/openssh_key.rs`、`structured.rs` | `parses_dsa_and_rsa_keys`、`rejects_invalid_key_blob`、`compares_openssh_public_keys`、smoke `ssh public key`/`ssh public key equal` | 已完成（base64 wire blob、RSA/DSA/ECDSA/Ed25519 字段、bits、comment） |
| diffoscope | 字体 | `diff/font.rs`、`engine.rs` | `extracts_font_metadata_and_advances`、`extracts_woff_metadata`、`extracts_woff2_metadata`、`rejects_non_font_bytes`、`compares_fonts_semantically`、`compares_woff_fonts_semantically`、`compares_woff2_fonts_semantically`、`invalid_font_falls_back_to_binary`、smoke `font semantic`/`font woff`/`font woff2` | 已完成（TTF/OTF/TTC/WOFF/WOFF2；家族/样式/字重/宽度/glyph 数/ASCII advance） |
| diffoscope | 报告与呈现器解耦 | `crates/diff-core/src/model.rs`、`render/` | CLI JSON/HTML/Markdown/Unified 测试 | 已完成 |
| diffoscope | 外部工具 comparator | `crates/diff-cli/src/main.rs` 的 `--external-tool/--external-arg` | `external_tool_delegates_and_propagates_status`、smoke `external comparator` | 已完成（stdout/stderr 透传，0/1/其他退出码） |
| diffoscope | 资源限制与递归深度 | `DiffOptions`、archive/ISO/CPIO/RPM 读取器 | `archive_entry_limit_is_enforced`、`invalid_package_and_image_formats_fall_back_to_binary` | 已完成 |
| Graphtage | 统一语义树 | `crates/diff-core/src/diff/structured.rs` | `compares_yaml_toml_ini_and_xml_semantically`、`compares_html_semantically` | 已完成 |
| Graphtage | 跨格式比较 | `FileKind::Structured`、`effective_kind` | `compares_json_against_yaml`、`compares_xml_and_binary_plists`、`compares_pickles_and_cross_encoding` | 已完成 |
| Graphtage | JSON/YAML/TOML/INI/XML/CSV | `structured.rs`、`table.rs` | `compares_csv_with_key_column`、`compares_tsv_tables` | 已完成 |
| Graphtage | 数组序列对齐 | `structured.rs` 的 `diff_array` | `aligns_inserted_array_items_without_cascading_changes`、`aligns_removed_array_items_without_cascading_changes`、`keeps_in_place_scalar_array_changes`、`reports_array_reordering_instead_of_hiding_it`、`aligns_inserted_json_array_items`、smoke `json array alignment` | 已完成（相似度动态规划，大数组回退 Myers） |
| Graphtage | plist / pickle | `parse_plist`、`parse_pickle` | `compares_xml_and_binary_plists`、`compares_pickles_and_cross_encoding`、`compares_pickles_with_unknown_globals_readably`、`invalid_pickles_fall_back_to_readable_objects` | 已完成（含 binary plist、pickle 4/5、未知 global 可读字符串、损坏 pickle 可读错误） |
| diffoscope | Gettext `.mo` 目录 | `diff/gettext.rs`、`structured.rs` | `parses_headers_and_messages`、`decodes_declared_charset`、`parses_plural_forms_and_context`、`rejects_truncated_catalog`、`compares_gettext_catalogs`、`compares_gettext_plural_forms`、smoke `gettext semantic`/`gettext equal`/`gettext plural` | 已完成（msgid/msgstr/复数/context、header map、charset 解码 UTF-8/ISO-8859-1） |
| difftastic | tree-sitter token diff | `crates/diff-core/src/diff/tokens.rs` | `tokenizes_rust`、`ignores_comments_when_requested`、smoke `token mode` | 已完成 |
| difftastic | 完整语法树 diff | `crates/diff-core/src/diff/tree_diff.rs` | `builds_and_compares_trees`、`aligns_identical_siblings_by_content`、`matches_replacement_siblings_by_similarity`、`matches_moved_siblings_across_positions`、`optimal_child_matching_beats_greedy`、`slider_orders_tied_matches_without_losing_score`、`comments_can_be_ignored`、smoke `tree mode` | 已完成（内容签名 + 最优二分匹配（匈牙利）+ 顺序滑动归一化，大节点 Myers 兜底） |
| difftastic | 语法图最短路径 | `tree_diff::compare_trees_graph`、`--mode graph` | `graph_mode_reports_precise_leaf_change`、`graph_mode_reports_added_statement`、`graph_mode_slides_repeated_arguments`、`graph_mode_aligns_inserted_repeated_argument`、`graph_mode_flags_reordered_statements`、`graph_mode_reports_moved_statement`、`graph_mode_reports_moved_and_modified_statement`、`graph_mode_keeps_inner_call_when_wrapped`、`graph_mode_keeps_inner_token_when_wrapped`、`graph_mode_keeps_existing_call_before_inserted_argument`、`graph_mode_highlights_changed_delimiters`、`graph_mode_handles_nested_repeated_calls`、`graph_mode_moves_nested_block`、`graph_mode_groups_removed_subtree`、`graph_mode_groups_added_function`、`graph_mode_groups_fully_modified_subtree`、`signature_lcs_prefers_minimal_index_shift`、`graph_mode_slides_duplicate_statements`、`graph_mode_reports_precise_changes`、`graph_mode_reports_moved_nodes`、smoke `graph mode` | 已完成（有界 enter/token/exit 事件图最短路径 + 非局部精确移动边 + 稳定子树锚点，深度无关 token 等价 + 节点类型代价 + 增删/修改子树分组 + LCS 移动检测（含最小索引位移滑块、移动后修改、嵌套重复调用/块）与 `Moved` 标记） |
| difftastic | 忽略注释 | `TokenizeOptions` | smoke `ignore comments auto` | 已完成 |
| difftastic | 多语言 | Rust/Python/JS/TS/C/C++/Go/Java/Bash/Ruby/C#/Swift/Lua | `tokens.rs` 单元测试 | 已完成（13 种） |
| Open Diff | 文本/表格/图片/Hex/二进制/目录统一入口 | `engine.rs` 分派 | 全量 formats/CLI 测试 | 已完成 |
| Open Diff | 三方合并（文本） | `crates/diff-core/src/merge.rs` | `three_way_merge_clean_and_conflict`、`merge_clean_and_conflict` | 已完成 |
| Open Diff | 三方合并（目录） | `crates/diff-core/src/folder_merge.rs` | `merges_changes_additions_deletions_and_conflicts`、`clean_merge_writes_only_changed_side`、`reports_file_directory_type_conflicts`、`folder_three_way_merge_with_conflicts`、`folder_three_way_merge_clean_exit_zero`、smoke `folder merge conflict/output` | 已完成（含文件/目录类型冲突报告） |
| Open Diff | 目录同步 | `crates/diff-core/src/sync.rs` | smoke `sync plan/apply/result`、`merge_clean_and_conflict` 之外的单测 | 已完成 |
| Open Diff | HTML/JSON/Markdown/Unified 报告 | `crates/diff-core/src/render/` | CLI 渲染测试、smoke `html output` / `markdown output` / `unified output` | 已完成 |
| Open Diff | 会话保存/加载 | `crates/diff-core/src/session.rs` | smoke `session save/load` | 已完成 |
| Open Diff | WebDAV/SFTP/SMB 远程输入 | `crates/diff-cli/src/remote.rs` | `webdav_remote_inputs_with_local_server`、`webdav_uses_basic_auth`、`sftp_remote_directories_with_local_server`、`smb_remote_inputs_with_local_server`、`rejects_unsafe_remote_file_names`、smoke 远程用例 | 已完成（目录递归 + Basic auth + known_hosts + 路径名过滤） |
| uni-diff | PDF/DOCX/XLSX/PPTX 文本化 | `documents.rs`、`workbook.rs` | `compares_pdf_text`、`extracts_pdf_text_with_page_markers`、`compares_docx_and_pptx_text`、`compares_workbooks`、smoke `pdf page markers` | 已完成（PDF 页级文本定位） |
| uni-diff | OCR | `ocr.rs` | smoke `ocr images`、`pdf ocr` | 已完成 |
| uni-diff | TUI | `crates/diff-cli/src/tui.rs` | smoke `tui fallback` | 已完成 |
| uni-diff | ANSI/JSON/HTML/PNG 输出 | `render/`、`image_export.rs`、`pdf_visual.rs` | 全量渲染/PDF 测试 | 已完成 |
| diff-pdf | PDF 页面视觉比较 | `pdf_visual.rs` | `pdf_visual_diff_when_pdftoppm_is_available`（含 report-vector.pdf 红色矩形渲染校验）、smoke `pdf visual` | 已完成（PNG/HTML/report.pdf/overlay/report-vector.pdf） |
| nbdime | Notebook cell diff | `crates/diff-core/src/diff/notebook.rs` | `compares_cells_by_id`、`reports_moved_cells`、`changed_cell_can_move_without_being_added_or_removed`、`detects_moves_for_cells_without_ids`、`moved_notebook_cells_are_different_without_add_remove`、smoke `notebook moved cells` | 已完成（id 全局对齐；无 id 用签名估算移动） |
| nbdime | Notebook 三方合并 | `notebook.rs` 合并路径 | `merge_preserves_ours_cell_order`、`merges_non_conflicting_cells`、`reports_source_conflicts`、`notebook_cell_merge` | 已完成 |
| HDiffPatch | 二进制 / 目录 delta | `crates/diff-core/src/patch.rs`、`folder_patch.rs`、CLI hdiffz 集成 | `pure_hdiff_encoder_roundtrip`、`zstd_hdiff_encoder_roundtrip`、`zlib_lzma2_bzip2_hdiff_encoders_roundtrip`、`fadler64_matches_official_directory_checksums`、`pure_dir_hdiff_roundtrip`、`zstd_dir_hdiff_roundtrip`、`zlib_lzma2_bzip2_dir_hdiff_roundtrip`、`creates_and_applies_pure_rust_hdiff`、`creates_and_applies_pure_rust_dir_hdiff`、`creates_and_applies_pure_rust_zstd_dir_hdiff`、`creates_and_applies_pure_rust_zlib_lzma2_bzip2_dir_hdiff`、`applies_official_dir_patch_with_pure_rust`、`uncompressed_hdiff_roundtrip`、`compressed_zstd_hdiff_roundtrip`、`compressed_lzma2_hdiff_roundtrip`、`applies_pure_rust_uncompressed_hdiff`、`applies_pure_rust_zstd_hdiff`、`applies_pure_rust_lzma2_hdiff`、`binary_patch_is_written`、`folder_patch_roundtrip_adds_modifies_and_deletes`、`folder_patch_create_and_apply`、smoke `hdiff rust`/`hdiff rust dir`/`hdiff rust dir zstd`/`hdiff rust dir lzma2`/`hdiff rust dir bzip2`/`hdiff official dir`/`hdiff patch` | 已完成（纯 Rust HDIFF13/HDIFF19 编码/应用、fadler64 校验、zstd/zlib/lzma2/bzip2 压缩、官方 zstd/校验目录 patch 应用、bsdiff、自定义目录 patch） |
| gocompare | 目录/表格/图片/文档统一 CLI | 全量 `engine.rs` | formats/CLI 测试 | 已完成 |
| gocompare | 先计划后执行的安全工作流 | `sync.rs` 计划/应用/备份 | smoke `sync plan/apply/result` | 已完成 |
| doc_diff_software | OFD 文本 | `documents.rs` | `compares_ofd_text`、smoke `ofd text` | 已完成 |
| Meld / KDiff3 | 文本三方合并 | `merge.rs` | 上述 merge 测试 | 已完成 |
| Meld / KDiff3 | 目录三方合并 | `folder_merge.rs` | 目录三方合并测试 + smoke | 已完成（保守策略） |
| diff-so-fancy | 人类可读 ANSI 布局 | `render/ansi.rs` | ANSI/CLI 渲染测试 | 已完成 |
| jsdiff / diff-match-patch | 行级/词级文本 diff | `diff/text.rs`（similar） | `annotates_changed_words` 等 | 已完成 |
| pixelmatch | 像素容差/差异计数 | `diff/image.rs`、`image_export.rs` | `detects_changed_pixel_and_box`、`finds_changed_rectangles`、`reports_multiple_changed_regions`、`merges_nearby_changes_into_coarse_regions`、`threshold_ignores_subtle_color_differences`、`image_threshold_can_ignore_changes`、`image_json_reports_changed_regions`、smoke `image pixels`/`image threshold`/`image regions` | 已完成（YIQ 感知容差 + 像素统计 + 块级 changed regions + 热力图/并排图） |
| openapi-diff | OpenAPI 语义 diff / breaking change | `structured.rs` 的 `compare_openapi` / `openapi_breaking` | `compares_openapi_breaking_changes`、smoke `openapi breaking changes` | 已完成（operation/path/response/必需参数标记） |
| csvdiff / daff | CSV 主键对齐与单元格 diff | `diff/table.rs` | `compares_csv_cells`、`aligns_columns_by_name`、`compares_csv_with_key_column`、`compares_tsv_tables` | 已完成 |
| diffsitter | tree-sitter 结构 diff | `tree_diff.rs` | `builds_and_compares_trees` | 已完成 |

## 新格式验证明细

| 格式 | 测试/夹具 | 命令 | 结果 |
|---|---|---|---|
| Plist XML / binary | `fixtures/plist/`、`compares_xml_and_binary_plists` | smoke `plist semantic/binary equal` | 通过 |
| Pickle protocol 4/5 | `fixtures/pickle/`、`compares_pickles_and_cross_encoding` | smoke `pickle semantic/protocol equal` | 通过 |
| Pickle unknown global / corrupt | `fixtures/pickle/old-global.pkl` / `new-global.pkl`、临时损坏文件 | `compares_pickles_with_unknown_globals_readably`、`invalid_pickles_fall_back_to_readable_objects`、smoke `pickle globals readable` | 通过（可读字符串/错误对象） |
| DEB | `fixtures/deb/`、真实 Debian hello 两个版本 | `compares_deb_packages_recursively`、smoke `deb package`/`external deb diff` | 通过（55 条目递归） |
| RPM | 真实 Fedora hello 两个版本 | `compares_real_rpm_packages`、smoke `external rpm diff` | 通过（54+ 条目递归 + 元数据） |
| RPM xz | 真实 CentOS which 两个版本 | `compares_real_xz_payload_rpm_packages`、smoke `external rpm xz diff` | 通过（xz payload 解码） |
| ISO9660 | `fixtures/iso/` | `compares_iso_images`、smoke `iso image/equal` | 通过 |
| CPIO newc | `fixtures/cpio/` | `compares_cpio_archives`、smoke `cpio archive/cpio.gz archive` | 通过 |
| TAR.ZST / ZST | `fixtures/archive-zst/` | `compares_tar_zst_archives`、smoke `tar.zst/zstd single file` | 通过（含跨压缩等值） |
| TAR.LZ4 / LZ4 | `fixtures/lz4/` | `compares_lz4_archives_and_single_files`、smoke `tar.lz4 archive/lz4 single file` | 通过 |
| BZ2 / XZ 单文件 | `fixtures/compression/` | `compares_single_bzip2_and_xz_files`、smoke `bzip2 single file/xz single file` | 通过 |
| ZIP bzip2/LZMA/zstd | `fixtures/zip-methods/` | `compares_zip_bzip2_lzma_and_zstd_entries`、smoke `zip zstd/lzma entry compression` | 通过 |
| OpenAPI breaking | `fixtures/openapi/` | `compares_openapi_breaking_changes`、smoke `openapi breaking changes` | 通过 |
| HDIFF13 uncompressed | `fixtures/hdiff/change.hdiff` | `uncompressed_hdiff_roundtrip`、`applies_pure_rust_uncompressed_hdiff`、smoke `hdiff pure rust apply/output` | 通过（纯 Rust） |
| HDIFF13 zstd | `fixtures/hdiff/large-zstd.hdiff` | `compressed_zstd_hdiff_roundtrip`、`applies_pure_rust_zstd_hdiff`、smoke `hdiff pure rust zstd apply/output` | 通过（纯 Rust） |
| HDIFF13 lzma2 | `fixtures/hdiff/large-lzma2.hdiff` | `compressed_lzma2_hdiff_roundtrip`、`applies_pure_rust_lzma2_hdiff`、smoke `hdiff pure rust lzma2 apply/output` | 通过（纯 Rust） |
| HDIFF13 encoder | 临时 old/new 与真实 hpatchz | `pure_hdiff_encoder_roundtrip`、`creates_and_applies_pure_rust_hdiff`、smoke `hdiff rust create/apply/output` | 通过（hpatchz 接受） |
| HDIFF19 directory | 临时 old/new 目录与真实 hpatchz/hdiffz | `fadler64_matches_official_directory_checksums`、`pure_dir_hdiff_roundtrip`、`zstd_dir_hdiff_roundtrip`、`zlib_lzma2_bzip2_dir_hdiff_roundtrip`、`creates_and_applies_pure_rust_dir_hdiff`、`creates_and_applies_pure_rust_zstd_dir_hdiff`、`creates_and_applies_pure_rust_zlib_lzma2_bzip2_dir_hdiff`、`applies_official_dir_patch_with_pure_rust`、smoke `hdiff rust dir`/`hdiff rust dir zstd`/`hdiff rust dir lzma2`/`hdiff rust dir bzip2`/`hdiff official dir` | 通过（校验编码 + zstd/zlib/lzma2/bzip2 编码 + 官方补丁应用 + hpatchz 校验） |
| SQLite | `fixtures/sqlite/old.db`、`new.db`、`old-copy.db` | `compares_sqlite_databases_semantically`、`invalid_sqlite_falls_back_to_binary`、smoke `sqlite semantic`/`sqlite page layout equal` | 通过（语义 diff + 物理布局等价） |
| ELF / PE / Mach-O | `fixtures/executable/hello-old`、`hello-new`；`diffoscope-test1.exe`、`test2.exe`；`diffoscope-test1.macho`、`test1-mod.macho`（diffoscope 数据） | `compares_elf_executables_semantically`、`compares_pe_executables_semantically`、`compares_pe_fixtures_semantically`、`compares_macho_executables_semantically`、smoke `elf semantic`/`pe semantic`/`pe fixture semantic`/`macho fat semantic` | 通过（节/符号/导入语义；fat Mach-O 按架构切片递归） |
| CAB | `fixtures/cab/old.cab`、`new.cab` | `compares_cab_archives`、`invalid_cab_falls_back_to_binary`、smoke `cab archive` | 通过（data.txt 修改 + extra.txt 新增） |
| XAR | `fixtures/xar/left.xar`、`right.xar`、`mixed-left.xar`、`mixed-right.xar`、`links-left.xar`、`links-right.xar` | `compares_xar_archives`、`compares_xar_compression_codecs`、`compares_xar_symlinks`、smoke `xar archive`/`xar equal`/`xar codecs`/`xar symlink` | 通过（zlib/bzip2/xz/lzma/raw 条目解码 + symlink 目标比较；`bsdtar`/libarchive 可列出并解出相同内容） |
| AR | `fixtures/ar/test1.a`、`test2.a`（diffoscope 测试库）；`bsd-old.a`、`bsd-new.a`（llvm-ar BSD 格式） | `compares_ar_archives`、`compares_bsd_ar_archives`、smoke `ar archive`/`bsd ar archive` | 通过（GNU test.o 递归为 ELF section digest diff；BSD `#1/` 长名成员文本 diff） |
| TAR links | `fixtures/archives/links-old.tar`、`links-new.tar` | `compares_tar_symlinks_and_hardlinks`、smoke `tar symlink` | 通过（symlink 目标变化 + hardlink 稳定） |
| Font | `fixtures/font/regular.ttf`、`bold.ttf`、`regular.woff`、`bold.woff`、`regular.woff2`、`bold.woff2`（Lato OFL） | `compares_fonts_semantically`、`compares_woff_fonts_semantically`、`compares_woff2_fonts_semantically`、`invalid_font_falls_back_to_binary`、smoke `font semantic`/`font woff`/`font woff2` | 通过（TTF/WOFF/WOFF2 元数据 + ASCII advance，跨格式判等） |
| Gettext | `fixtures/gettext/test1.mo`、`test2.mo`、`test_no_charset.mo`、`test_iso8859-1.mo`（diffoscope 数据）、`plural-old.mo`、`plural-new.mo` | `compares_gettext_catalogs`、smoke `gettext semantic`/`gettext equal` | 通过（catalog header/message diff，ISO-8859-1 正确解码） |
| WebAssembly | `fixtures/wasm/old.wasm`、`new.wasm`（rustc wasm32，Node 验证） | `compares_wasm_modules`、smoke `wasm semantic`/`wasm equal` | 通过（新增 `mul` export、`add` code digest 变化） |
| Java class | `fixtures/java/Test1.class`、`Test2.class`（diffoscope 数据）、`old.jar`、`new.jar` | `compares_java_class_files`、smoke `java class semantic`/`java class equal` | 通过（`main` 方法 bytecode digest 变化，`javap -c` 验证语义差异） |
| Android APK | `fixtures/apk/old.apk`、`new.apk`、`manifest1.axml`、`manifest2.axml`（diffoscope 数据） | `compares_apk_manifests_semantically`、smoke `apk manifest`/`apk manifest equal` | 通过（versionCode 1→2、versionName 1.0→2.0） |
| Git index | `fixtures/git/test1.git-index`、`test2.git-index`（diffoscope 数据） | `compares_git_indexes`、smoke `git index`/`git index equal` | 通过（新增路径 `b` 条目） |
| ICO / CUR | `fixtures/images/diffoscope-test1.ico`、`test2.ico`（diffoscope 数据） | `compares_ico_images`、smoke `ico semantic`/`ico equal` | 通过（16x16 4bpp vs 32bpp BMP 图标） |
| IPK | `fixtures/ipk/old.ipk`、`new.ipk`（diffoscope 数据） | `compares_ipk_packages_recursively`、smoke `ipk package`/`ipk package equal` | 通过（control 版本/Installed-Size、data 递归比较） |
| OpenSSH key | `fixtures/ssh/test_openssh_pub_key1.pub`、`test_openssh_pub_key2.pub`（diffoscope 数据） | `compares_openssh_public_keys`、smoke `ssh public key`/`ssh public key equal` | 通过（DSA 1024 → RSA 4096，字段级比较） |
| 真实 Arch `which` | `testdata/external/which-*.pkg.tar.zst` | smoke `external tar.zst equal/diff` | 通过 |
| 真实 DEB / RPM | `testdata/external/hello-*` | smoke `external deb/rpm equal/diff` | 通过 |

## 仍存在的边界

以下不是 CLI 功能缺陷，而是明确的范围取舍：

- GUI/Web 渲染不做（Meld、KDiff3、nbdime Web、Open Diff GUI 仅作参考）。
- difftastic 的完整 enter/exit Dijkstra 图搜索未逐行复刻；tree mode 使用内容签名、最优二分匹配和顺序滑动归一化，graph mode 使用有界语法事件图最短路径 + 非局部精确移动边、深度无关 token 等价、子树分组与 LCS 移动检测（含最小索引位移滑块），超大节点回退 Myers。
- 字体支持 TTF/OTF/TTC/WOFF/WOFF2；WOFF2 通过 Brotli 与 table transform 解包为 SFNT 后复用元数据比较。
- nbdime 的 Web 渲染不做；Notebook cell 已有 id 全局对齐，无 id 时用签名估算移动。
- uni-diff 的文档内字符/坐标级 block-level 视觉定位未完整复刻；PDF 已有页级文本定位，图片/PDF 已有粗粒度 changed-region 包围盒。
- OpenAPI breaking-change 已覆盖常见 operation/path/response/required 参数规则，复杂 schema 语义仍按通用 JSON 树呈现。
- HDiffPatch 单文件 HDIFF13 与目录 HDIFF19 的编码/应用已由纯 Rust 完成（目录编码含 fadler64 校验、same-file 去重、可选 zstd/zlib/lzma2/bzip2 head/hdiff 压缩）；应用侧可读取官方 zstd 压缩 head 和 fadler64 校验目录 patch。
- 普通 HTML 报告本身无外部资源；PDF 视觉 `report.html` 的页面图已内嵌为 base64 PNG。
- pickle 未知 global 已提取可读字符串；损坏 pickle 返回带 `$pickle_error`/`$bytes_hash`/`$readable_strings` 的结构化对象。

## 一键验证

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\verify-all.ps1
```

该脚本依次运行 fmt check、clippy、全量测试、release 构建和 189 项 smoke。
