use serde_json::Value;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

fn fixture(relative: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.join("fixtures").join(relative)
}

fn run(args: &[&Path]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_diff-everything"));
    for arg in args {
        command.arg(arg);
    }
    command.output().expect("failed to run diff-everything")
}

fn run_str(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_diff-everything"))
        .args(args)
        .output()
        .expect("failed to run diff-everything")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn head_last_and_filter_select_text_content() {
    let dir = tempfile::tempdir().unwrap();
    let old_path = dir.path().join("old.txt");
    let new_path = dir.path().join("new.txt");
    std::fs::write(&old_path, "alpha\nbeta\ngamma\ndelta\n").unwrap();
    std::fs::write(&new_path, "alpha2\nbeta2\ngamma2\ndelta2\n").unwrap();
    let old = old_path.to_str().unwrap();
    let new = new_path.to_str().unwrap();

    let head = run_str(&[old, new, "--no-color", "--head", "2", "--context", "0"]);
    assert_eq!(head.status.code(), Some(1));
    let head_text = stdout(&head);
    assert!(head_text.contains("-alpha"), "{head_text}");
    assert!(head_text.contains("-beta"), "{head_text}");
    assert!(!head_text.contains("delta"), "{head_text}");
    assert!(head_text.contains("(truncated)"), "{head_text}");

    let last = run_str(&[old, new, "--no-color", "--last", "2", "--context", "0"]);
    let last_text = stdout(&last);
    assert!(last_text.contains("+gamma2"), "{last_text}");
    assert!(last_text.contains("+delta2"), "{last_text}");
    assert!(!last_text.contains("alpha"), "{last_text}");

    let filtered = run_str(&[
        old,
        new,
        "--no-color",
        "--filter",
        "gamma",
        "--context",
        "0",
    ]);
    let filtered_text = stdout(&filtered);
    assert!(filtered_text.contains("-gamma"), "{filtered_text}");
    assert!(filtered_text.contains("+gamma2"), "{filtered_text}");
    assert!(!filtered_text.contains("alpha"), "{filtered_text}");
    assert!(!filtered_text.contains("delta"), "{filtered_text}");
}

#[test]
fn filter_selects_paths_in_directories() {
    let output = run_str(&[
        fixture("dir/left").to_str().unwrap(),
        fixture("dir/right").to_str().unwrap(),
        "--no-color",
        "--filter",
        "README",
    ]);
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(text.contains("README.md"), "{text}");
    assert!(!text.contains("main.rs"), "{text}");
}

#[test]
fn filter_limits_json_hunks_and_head_rejects_zero() {
    let dir = tempfile::tempdir().unwrap();
    let old_path = dir.path().join("old.txt");
    let new_path = dir.path().join("new.txt");
    std::fs::write(&old_path, "alpha\nbeta\ngamma\n").unwrap();
    std::fs::write(&new_path, "alpha2\nbeta2\ngamma2\n").unwrap();
    let output = run_str(&[
        old_path.to_str().unwrap(),
        new_path.to_str().unwrap(),
        "--format",
        "json",
        "--filter",
        "gamma",
        "--context",
        "0",
    ]);
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    let lines = value["root"]["body"]["hunks"][0]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|line| line["text"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(lines, vec!["gamma", "gamma2"]);

    let zero = run_str(&[
        old_path.to_str().unwrap(),
        new_path.to_str().unwrap(),
        "--head",
        "0",
    ]);
    assert_eq!(zero.status.code(), Some(2));
    assert!(stderr(&zero).contains("at least 1"), "{}", stderr(&zero));
}

#[test]
fn concise_is_the_default_detail_mode() {
    let dir = tempfile::tempdir().unwrap();
    let old_path = dir.path().join("old.txt");
    let new_path = dir.path().join("new.txt");
    std::fs::write(&old_path, "DEFAULT-CONTEXT\nold\n").unwrap();
    std::fs::write(&new_path, "DEFAULT-CONTEXT\nnew\n").unwrap();
    let output = run_str(&[
        old_path.to_str().unwrap(),
        new_path.to_str().unwrap(),
        "--no-color",
    ]);
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(!text.contains("DEFAULT-CONTEXT"), "{text}");
    assert!(text.contains("-old"), "{text}");
    assert!(text.contains("+new"), "{text}");
}

#[test]
fn full_detail_decompiles_apk_dex_with_yes() {
    let output = run_str(&[
        fixture("apk/code-old.apk").to_str().unwrap(),
        fixture("apk/code-new.apk").to_str().unwrap(),
        "--detail",
        "full",
        "-y",
        "--no-color",
    ]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("classes.dex dex-java"), "{text}");
    assert!(text.contains("onCreate"), "{text}");
}

#[test]
fn full_detail_decompiles_apk_native_libs() {
    let output = run_str(&[
        fixture("apk/native-old.apk").to_str().unwrap(),
        fixture("apk/native-new.apk").to_str().unwrap(),
        "--detail",
        "full",
        "-y",
        "--no-color",
    ]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("lib/x86_64/libhello.so"), "{text}");
    assert!(text.contains("decompiled code (embedded)"), "{text}");
    assert!(text.contains("Disassembly of section .text:"), "{text}");
}

#[test]
fn full_detail_decompiles_executables_with_yes() {
    let output = run_str(&[
        fixture("executable/hello-old").to_str().unwrap(),
        fixture("executable/hello-new").to_str().unwrap(),
        "--detail",
        "full",
        "-y",
        "--no-color",
    ]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("decompiled code"), "{text}");
    assert!(text.contains("Disassembly of section .text:"), "{text}");
}

#[test]
fn full_detail_without_yes_skips_decompile_when_noninteractive() {
    let output = run_str(&[
        fixture("executable/hello-old").to_str().unwrap(),
        fixture("executable/hello-new").to_str().unwrap(),
        "--detail",
        "full",
        "--no-color",
    ]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(!stdout(&output).contains("decompiled code"));
    assert!(
        stderr(&output).contains("stdin is not a terminal"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn concise_detail_suppresses_equal_context() {
    let dir = tempfile::tempdir().unwrap();
    let old_path = dir.path().join("old.txt");
    let new_path = dir.path().join("new.txt");
    std::fs::write(
        &old_path,
        "KEEP-CONTEXT-LINE\nold-value\nKEEP-CONTEXT-LINE-2\n",
    )
    .unwrap();
    std::fs::write(
        &new_path,
        "KEEP-CONTEXT-LINE\nnew-value\nKEEP-CONTEXT-LINE-2\n",
    )
    .unwrap();
    let old = old_path.to_str().unwrap();
    let new = new_path.to_str().unwrap();

    let full = run_str(&[old, new, "--no-color", "--detail", "full"]);
    let full_text = stdout(&full);
    assert!(full_text.contains("KEEP-CONTEXT-LINE"), "{full_text}");

    let concise = run_str(&[old, new, "--no-color", "--detail", "concise"]);
    let concise_text = stdout(&concise);
    assert!(
        !concise_text.contains("KEEP-CONTEXT-LINE"),
        "{concise_text}"
    );
    assert!(concise_text.contains("-old-value"), "{concise_text}");
    assert!(concise_text.contains("+new-value"), "{concise_text}");
    assert!(
        concise_text.lines().count() < full_text.lines().count(),
        "concise should be shorter"
    );
}

fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write as _;
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, data) in entries {
        writer.start_file(*name, options).unwrap();
        writer.write_all(data).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[test]
fn max_depth_limits_nested_archives_and_validates_zero() {
    let dir = tempfile::tempdir().unwrap();
    let inner_left = zip_bytes(&[(
        "a.txt", b"old
",
    )]);
    let inner_right = zip_bytes(&[(
        "a.txt", b"new
",
    )]);
    let left = dir.path().join("left.zip");
    let right = dir.path().join("right.zip");
    std::fs::write(&left, zip_bytes(&[("inner.zip", &inner_left)])).unwrap();
    std::fs::write(&right, zip_bytes(&[("inner.zip", &inner_right)])).unwrap();

    let limited = run_str(&[
        left.to_str().unwrap(),
        right.to_str().unwrap(),
        "--detail",
        "full",
        "--max-depth",
        "1",
        "--no-color",
    ]);
    assert_eq!(limited.status.code(), Some(1), "{}", stderr(&limited));
    let text = stdout(&limited);
    assert!(
        text.contains("nested archive depth limit reached"),
        "{text}"
    );

    let zero = run_str(&[
        left.to_str().unwrap(),
        right.to_str().unwrap(),
        "--max-depth",
        "0",
    ]);
    assert_eq!(zero.status.code(), Some(2));
    assert!(
        stderr(&zero).contains("between 1 and 1024"),
        "{}",
        stderr(&zero)
    );
}

#[test]
fn exit_codes_match_diff_convention() {
    let equal = run(&[&fixture("text/old.txt"), &fixture("text/old.txt")]);
    assert_eq!(equal.status.code(), Some(0));

    let different = run(&[&fixture("text/old.txt"), &fixture("text/new.txt")]);
    assert_eq!(different.status.code(), Some(1));

    let missing = run(&[Path::new("does-not-exist"), &fixture("text/new.txt")]);
    assert_eq!(missing.status.code(), Some(2));
    assert!(stderr(&missing).contains("I/O error"));
}

#[test]
fn sync_rejects_remote_inputs() {
    let right = fixture("dir/right");
    let output = run_str(&[
        "webdav://example.invalid/left",
        right.to_str().unwrap(),
        "--sync",
        "mirror-left-to-right",
        "--no-color",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("local paths only"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn json_output_is_machine_readable() {
    let output = run_str(&[
        fixture("json/old.json").to_str().unwrap(),
        fixture("json/new.json").to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(output.status.code(), Some(1));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["status"], "different");
    assert_eq!(value["root"]["body"]["type"], "structured");
    assert_eq!(value["summary"]["structured_changes"], 5);
}

#[test]
fn aligns_inserted_json_array_items() {
    let dir = tempfile::tempdir().unwrap();
    let old_path = dir.path().join("old.json");
    let new_path = dir.path().join("new.json");
    std::fs::write(
        &old_path,
        r#"[{"id":1,"name":"alpha"},{"id":2,"name":"beta"},{"id":3,"name":"gamma"}]"#,
    )
    .unwrap();
    std::fs::write(
        &new_path,
        r#"[{"id":0,"name":"zero"},{"id":1,"name":"alpha"},{"id":2,"name":"beta"},{"id":3,"name":"gamma"}]"#,
    )
    .unwrap();
    let output = run(&[
        &old_path,
        &new_path,
        Path::new("--format"),
        Path::new("json"),
        Path::new("--no-color"),
    ]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["summary"]["structured_changes"], 1);
    let changes = report["root"]["body"]["changes"].as_array().unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0]["kind"], "added");
    assert_eq!(changes[0]["path"], "$[0]");
    assert_eq!(changes[0]["right"]["id"], 0);
}

#[test]
fn graph_mode_reports_precise_changes() {
    let output = run(&[
        &fixture("code/old.rs"),
        &fixture("code/new.rs"),
        Path::new("--mode"),
        Path::new("graph"),
        Path::new("--format"),
        Path::new("json"),
        Path::new("--no-color"),
    ]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["root"]["body"]["type"], "tree");
    assert!(report["root"]["body"]["modified_nodes"].as_u64().unwrap() > 0);
    assert!(
        report["root"]["body"]["chunks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|chunk| chunk["text"]
                .as_str()
                .is_some_and(|text| text.contains("1 -> 2")))
    );
}

#[test]
fn graph_mode_reports_moved_nodes() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old.rs");
    let new = dir.path().join("new.rs");
    std::fs::write(&old, "fn main() { foo(); bar(); baz(); }").unwrap();
    std::fs::write(&new, "fn main() { baz(); foo(); bar(); }").unwrap();
    let output = run(&[
        &old,
        &new,
        Path::new("--mode"),
        Path::new("graph"),
        Path::new("--format"),
        Path::new("json"),
        Path::new("--no-color"),
    ]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report["root"]["body"]["moved_nodes"].as_u64().unwrap() > 0);
    assert!(
        report["root"]["body"]["chunks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|chunk| chunk["tag"] == "moved")
    );
}

#[test]
fn name_only_and_quiet() {
    let name_only = run_str(&[
        fixture("dir/left").to_str().unwrap(),
        fixture("dir/right").to_str().unwrap(),
        "--name-only",
    ]);
    assert_eq!(name_only.status.code(), Some(1));
    let text = stdout(&name_only);
    assert!(text.contains("README.md"));
    assert!(text.contains("new-only.txt"));
    assert!(!text.contains("Summary"));

    let quiet = run_str(&[
        fixture("text/old.txt").to_str().unwrap(),
        fixture("text/new.txt").to_str().unwrap(),
        "--quiet",
    ]);
    assert_eq!(quiet.status.code(), Some(1));
    assert!(quiet.stdout.is_empty());
}

#[test]
fn token_mode_ignores_comments_and_formatting() {
    let output = run_str(&[
        fixture("code/old.rs").to_str().unwrap(),
        fixture("code/new.rs").to_str().unwrap(),
        "--mode",
        "token",
        "--ignore-comments",
        "--no-color",
    ]);
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(text.contains("rust tokens: +1 -1"), "{text}");
    assert!(text.contains("-1"));
    assert!(text.contains("+2"));
    assert!(!text.contains("+2 -2"));
}

#[test]
fn csv_key_and_excludes() {
    let csv = run_str(&[
        fixture("table/old.csv").to_str().unwrap(),
        fixture("table/new.csv").to_str().unwrap(),
        "--key",
        "id",
        "--no-color",
    ]);
    let text = stdout(&csv);
    assert!(text.contains("~ 1: age: 30 -> 31"), "{text}");
    assert!(text.contains("- 3:"), "{text}");
    assert!(text.contains("+ 4:"), "{text}");

    let excluded = run_str(&[
        fixture("dir/left").to_str().unwrap(),
        fixture("dir/right").to_str().unwrap(),
        "--exclude",
        "*.txt",
        "--no-color",
    ]);
    let text = stdout(&excluded);
    assert!(!text.contains(".txt"), "{text}");
    assert!(text.contains("README.md"), "{text}");
}

#[test]
fn image_heatmap_and_side_by_side_are_written() {
    let dir = tempfile::tempdir().unwrap();
    let heatmap = dir.path().join("heatmap.png");
    let side_by_side = dir.path().join("side-by-side.png");
    let output = run(&[
        &fixture("images/old.png"),
        &fixture("images/new.png"),
        Path::new("--image-heatmap"),
        &heatmap,
        Path::new("--image-side-by-side"),
        &side_by_side,
        Path::new("--quiet"),
    ]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let heatmap_bytes = std::fs::read(&heatmap).unwrap();
    assert!(heatmap_bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert!(heatmap_bytes.len() > 100);
    let side_bytes = std::fs::read(&side_by_side).unwrap();
    assert!(side_bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
}

#[test]
fn image_json_reports_changed_regions() {
    let output = run(&[
        &fixture("images/old.png"),
        &fixture("images/new.png"),
        Path::new("--format"),
        Path::new("json"),
        Path::new("--no-color"),
        Path::new("--detail"),
        Path::new("full"),
    ]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let regions = report["root"]["body"]["regions"].as_array().unwrap();
    assert!(!regions.is_empty());
    assert!(regions[0]["width"].as_u64().unwrap() > 0);
    assert_eq!(report["root"]["body"]["regions_truncated"], false);
}

#[test]
fn image_threshold_can_ignore_changes() {
    let tolerant = run(&[
        &fixture("images/old.png"),
        &fixture("images/new.png"),
        Path::new("--image-threshold"),
        Path::new("1.0"),
        Path::new("--quiet"),
    ]);
    assert_eq!(tolerant.status.code(), Some(0), "{}", stderr(&tolerant));

    let invalid = run(&[
        &fixture("images/old.png"),
        &fixture("images/new.png"),
        Path::new("--image-threshold"),
        Path::new("2"),
    ]);
    assert_eq!(invalid.status.code(), Some(2));
    assert!(
        stderr(&invalid).contains("between 0 and 1"),
        "{}",
        stderr(&invalid)
    );
}

#[test]
fn ignore_whitespace_and_case() {
    let dir = tempfile::tempdir().unwrap();
    let left = dir.path().join("left.txt");
    let right = dir.path().join("right.txt");
    std::fs::write(&left, "Hello\n    value = 1\n").unwrap();
    std::fs::write(&right, "hello\n\tvalue = 1\n").unwrap();

    let strict = run(&[&left, &right, Path::new("--quiet")]);
    assert_eq!(strict.status.code(), Some(1));

    let relaxed = run(&[
        &left,
        &right,
        Path::new("--ignore-whitespace"),
        Path::new("--ignore-case"),
        Path::new("--quiet"),
    ]);
    assert_eq!(relaxed.status.code(), Some(0), "{}", stderr(&relaxed));
}

#[test]
fn three_way_merge_clean_and_conflict() {
    let clean = run_str(&[
        fixture("merge/left.txt").to_str().unwrap(),
        fixture("merge/right.txt").to_str().unwrap(),
        "--base",
        fixture("merge/base.txt").to_str().unwrap(),
    ]);
    assert_eq!(clean.status.code(), Some(0));
    let text = stdout(&clean);
    assert!(text.contains("left one"));
    assert!(text.contains("right three"));

    let conflict = run_str(&[
        fixture("merge/conflict_left.txt").to_str().unwrap(),
        fixture("merge/conflict_right.txt").to_str().unwrap(),
        "--base",
        fixture("merge/base.txt").to_str().unwrap(),
    ]);
    assert_eq!(conflict.status.code(), Some(1));
    let text = stdout(&conflict);
    assert!(text.contains("<<<<<<< ours"));
    assert!(text.contains(">>>>>>> theirs"));
}

#[test]
fn folder_three_way_merge_with_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("base");
    let ours = dir.path().join("ours");
    let theirs = dir.path().join("theirs");
    let merged = dir.path().join("merged");
    for root in [&base, &ours, &theirs] {
        std::fs::create_dir_all(root).unwrap();
    }
    for root in [&base, &ours, &theirs] {
        std::fs::write(root.join("clean.txt"), "base\n").unwrap();
        std::fs::write(root.join("both.txt"), "base\n").unwrap();
    }
    std::fs::write(ours.join("clean.txt"), "base\n").unwrap();
    std::fs::write(theirs.join("clean.txt"), "theirs\n").unwrap();
    std::fs::write(ours.join("both.txt"), "ours\n").unwrap();
    std::fs::write(theirs.join("both.txt"), "theirs\n").unwrap();

    let output = run_str(&[
        ours.to_str().unwrap(),
        theirs.to_str().unwrap(),
        "--base",
        base.to_str().unwrap(),
        "--merge-output",
        merged.to_str().unwrap(),
        "--no-color",
    ]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("folder merge:"), "{text}");
    assert!(text.contains("conflict: both.txt"), "{text}");
    assert_eq!(
        std::fs::read_to_string(merged.join("clean.txt")).unwrap(),
        "theirs\n"
    );
    let conflict = std::fs::read_to_string(merged.join("both.txt")).unwrap();
    assert!(conflict.contains("<<<<<<< ours\nours\n"), "{conflict}");
    assert!(conflict.contains(">>>>>>> theirs\n"), "{conflict}");
}

#[test]
fn folder_three_way_merge_clean_exit_zero() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("base");
    let ours = dir.path().join("ours");
    let theirs = dir.path().join("theirs");
    let merged = dir.path().join("merged");
    for root in [&base, &ours, &theirs] {
        std::fs::create_dir_all(root).unwrap();
    }
    std::fs::write(base.join("a.txt"), "base\n").unwrap();
    std::fs::write(ours.join("a.txt"), "ours\n").unwrap();
    std::fs::write(theirs.join("a.txt"), "base\n").unwrap();

    let output = run_str(&[
        ours.to_str().unwrap(),
        theirs.to_str().unwrap(),
        "--base",
        base.to_str().unwrap(),
        "--merge-output",
        merged.to_str().unwrap(),
        "--no-color",
    ]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        std::fs::read_to_string(merged.join("a.txt")).unwrap(),
        "ours\n"
    );
    assert!(
        !stdout(&output).contains("conflict:"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn creates_and_applies_pure_rust_hdiff() {
    let dir = tempfile::tempdir().unwrap();
    let patch_path = dir.path().join("rust.hdiff");
    let output_path = dir.path().join("applied.bin");
    let created = run(&[
        &fixture("binary/old.bin"),
        &fixture("binary/new.bin"),
        Path::new("--patch-format"),
        Path::new("hdiff-rust"),
        Path::new("--patch-output"),
        &patch_path,
        Path::new("--no-color"),
    ]);
    assert_eq!(created.status.code(), Some(0), "{}", stderr(&created));
    let applied = run(&[
        &fixture("binary/old.bin"),
        Path::new("--apply-patch"),
        &patch_path,
        Path::new("--merge-output"),
        &output_path,
        Path::new("--no-color"),
    ]);
    assert_eq!(applied.status.code(), Some(0), "{}", stderr(&applied));
    let expected = std::fs::read(fixture("binary/new.bin")).unwrap();
    assert_eq!(std::fs::read(&output_path).unwrap(), expected);

    if let Some(hpatchz) = local_tool("hpatchz") {
        let hpatchz_output = dir.path().join("hpatchz-applied.bin");
        let status = Command::new(hpatchz)
            .args([
                "-f",
                fixture("binary/old.bin").to_str().unwrap(),
                patch_path.to_str().unwrap(),
                hpatchz_output.to_str().unwrap(),
            ])
            .status()
            .unwrap();
        assert!(status.success(), "hpatchz rejected the Rust-created patch");
        assert_eq!(std::fs::read(hpatchz_output).unwrap(), expected);
    }
}

#[test]
fn creates_and_applies_pure_rust_dir_hdiff() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old");
    let new = dir.path().join("new");
    let out = dir.path().join("out");
    let patch_path = dir.path().join("dir.hdiff");
    std::fs::create_dir_all(old.join("nested")).unwrap();
    std::fs::create_dir_all(new.join("sub")).unwrap();
    std::fs::write(old.join("keep.txt"), b"old").unwrap();
    std::fs::write(old.join("same.txt"), b"same").unwrap();
    std::fs::write(old.join("remove.txt"), b"remove").unwrap();
    std::fs::write(old.join("nested/old.txt"), b"old nested").unwrap();
    std::fs::write(new.join("keep.txt"), b"new").unwrap();
    std::fs::write(new.join("same.txt"), b"same").unwrap();
    std::fs::write(new.join("added.txt"), b"added").unwrap();
    std::fs::write(new.join("sub/deep.txt"), b"deep").unwrap();

    let created = run(&[
        &old,
        &new,
        Path::new("--patch-format"),
        Path::new("hdiff-rust"),
        Path::new("--patch-output"),
        &patch_path,
        Path::new("--no-color"),
    ]);
    assert_eq!(created.status.code(), Some(0), "{}", stderr(&created));
    let patch_bytes = std::fs::read(&patch_path).unwrap();
    assert!(patch_bytes.starts_with(b"HDIFF19&&fadler64\0"));

    let applied = run(&[
        &old,
        Path::new("--apply-patch"),
        &patch_path,
        Path::new("--merge-output"),
        &out,
        Path::new("--no-color"),
    ]);
    assert_eq!(applied.status.code(), Some(0), "{}", stderr(&applied));
    assert_eq!(std::fs::read(out.join("keep.txt")).unwrap(), b"new");
    assert_eq!(std::fs::read(out.join("same.txt")).unwrap(), b"same");
    assert_eq!(std::fs::read(out.join("added.txt")).unwrap(), b"added");
    assert_eq!(std::fs::read(out.join("sub/deep.txt")).unwrap(), b"deep");
    assert!(!out.join("remove.txt").exists());
    assert!(!out.join("nested/old.txt").exists());

    if let Some(hpatchz) = local_tool("hpatchz") {
        let hpatchz_out = dir.path().join("hpatchz-out");
        let status = Command::new(hpatchz)
            .args([
                "-f",
                old.to_str().unwrap(),
                patch_path.to_str().unwrap(),
                hpatchz_out.to_str().unwrap(),
            ])
            .status()
            .unwrap();
        assert!(
            status.success(),
            "hpatchz rejected the Rust-created dir patch"
        );
        assert_eq!(std::fs::read(hpatchz_out.join("keep.txt")).unwrap(), b"new");
        assert_eq!(
            std::fs::read(hpatchz_out.join("same.txt")).unwrap(),
            b"same"
        );
        assert_eq!(
            std::fs::read(hpatchz_out.join("sub/deep.txt")).unwrap(),
            b"deep"
        );
        assert!(!hpatchz_out.join("remove.txt").exists());
    }
}

#[test]
fn creates_and_applies_pure_rust_zstd_dir_hdiff() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old");
    let new = dir.path().join("new");
    let out = dir.path().join("out");
    let patch_path = dir.path().join("dir-zstd.hdiff");
    std::fs::create_dir_all(old.join("nested")).unwrap();
    std::fs::create_dir_all(new.join("sub")).unwrap();
    std::fs::write(old.join("keep.txt"), b"old").unwrap();
    std::fs::write(old.join("same.txt"), b"same").unwrap();
    std::fs::write(old.join("remove.txt"), b"remove").unwrap();
    std::fs::write(old.join("nested/old.txt"), b"old nested").unwrap();
    std::fs::write(new.join("keep.txt"), b"new").unwrap();
    std::fs::write(new.join("same.txt"), b"same").unwrap();
    std::fs::write(new.join("added.txt"), b"added").unwrap();
    std::fs::write(new.join("sub/deep.txt"), b"deep").unwrap();

    let created = run(&[
        &old,
        &new,
        Path::new("--patch-format"),
        Path::new("hdiff-rust"),
        Path::new("--patch-compress"),
        Path::new("zstd"),
        Path::new("--patch-output"),
        &patch_path,
        Path::new("--no-color"),
    ]);
    assert_eq!(created.status.code(), Some(0), "{}", stderr(&created));
    let patch_bytes = std::fs::read(&patch_path).unwrap();
    assert!(patch_bytes.starts_with(b"HDIFF19&zstd&fadler64\0"));

    let applied = run(&[
        &old,
        Path::new("--apply-patch"),
        &patch_path,
        Path::new("--merge-output"),
        &out,
        Path::new("--no-color"),
    ]);
    assert_eq!(applied.status.code(), Some(0), "{}", stderr(&applied));
    assert_eq!(std::fs::read(out.join("keep.txt")).unwrap(), b"new");
    assert_eq!(std::fs::read(out.join("same.txt")).unwrap(), b"same");
    assert_eq!(std::fs::read(out.join("sub/deep.txt")).unwrap(), b"deep");
    assert!(!out.join("remove.txt").exists());

    if let Some(hpatchz) = local_tool("hpatchz") {
        let hpatchz_out = dir.path().join("hpatchz-zstd-out");
        let status = Command::new(hpatchz)
            .args([
                "-f",
                old.to_str().unwrap(),
                patch_path.to_str().unwrap(),
                hpatchz_out.to_str().unwrap(),
            ])
            .status()
            .unwrap();
        assert!(
            status.success(),
            "hpatchz rejected the Rust-created zstd dir patch"
        );
        assert_eq!(std::fs::read(hpatchz_out.join("keep.txt")).unwrap(), b"new");
    }
}

#[test]
fn creates_and_applies_pure_rust_zlib_lzma2_bzip2_dir_hdiff() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old");
    let new = dir.path().join("new");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::create_dir_all(&new).unwrap();
    std::fs::write(old.join("keep.txt"), b"old").unwrap();
    std::fs::write(new.join("keep.txt"), b"new").unwrap();
    std::fs::write(new.join("large.txt"), vec![b'L'; 50_000]).unwrap();

    for compression in ["zlib", "lzma2", "bzip2"] {
        let patch_path = dir.path().join(format!("dir-{compression}.hdiff"));
        let out = dir.path().join(format!("out-{compression}"));
        let created = run(&[
            &old,
            &new,
            Path::new("--patch-format"),
            Path::new("hdiff-rust"),
            Path::new("--patch-compress"),
            Path::new(compression),
            Path::new("--patch-output"),
            &patch_path,
            Path::new("--no-color"),
        ]);
        assert_eq!(created.status.code(), Some(0), "{}", stderr(&created));
        let patch_bytes = std::fs::read(&patch_path).unwrap();
        assert!(
            patch_bytes.starts_with(format!("HDIFF19&{compression}&fadler64\0").as_bytes()),
            "{compression}"
        );
        let applied = run(&[
            &old,
            Path::new("--apply-patch"),
            &patch_path,
            Path::new("--merge-output"),
            &out,
            Path::new("--no-color"),
        ]);
        assert_eq!(applied.status.code(), Some(0), "{}", stderr(&applied));
        assert_eq!(std::fs::read(out.join("keep.txt")).unwrap(), b"new");
        assert_eq!(std::fs::read(out.join("large.txt")).unwrap().len(), 50_000);

        if (compression == "lzma2" || compression == "bzip2")
            && let Some(hpatchz) = local_tool("hpatchz")
        {
            let hpatchz_out = dir.path().join(format!("hpatchz-{compression}-out"));
            let status = Command::new(hpatchz)
                .args([
                    "-f",
                    old.to_str().unwrap(),
                    patch_path.to_str().unwrap(),
                    hpatchz_out.to_str().unwrap(),
                ])
                .status()
                .unwrap();
            assert!(
                status.success(),
                "hpatchz rejected the Rust-created {compression} patch"
            );
            assert_eq!(
                std::fs::read(hpatchz_out.join("large.txt")).unwrap().len(),
                50_000
            );
        }
    }
}

#[test]
fn applies_official_dir_patch_with_pure_rust() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old");
    let new = dir.path().join("new");
    std::fs::create_dir_all(old.join("nested")).unwrap();
    std::fs::create_dir_all(new.join("sub")).unwrap();
    std::fs::write(old.join("keep.txt"), b"old").unwrap();
    std::fs::write(old.join("same.txt"), b"same").unwrap();
    std::fs::write(old.join("remove.txt"), b"remove").unwrap();
    std::fs::write(old.join("nested/old.txt"), b"old nested").unwrap();
    std::fs::write(new.join("keep.txt"), b"new").unwrap();
    std::fs::write(new.join("same.txt"), b"same").unwrap();
    std::fs::write(new.join("added.txt"), b"added").unwrap();
    std::fs::write(new.join("sub/deep.txt"), b"deep").unwrap();

    let Some(hdiffz) = local_tool("hdiffz") else {
        return;
    };
    let patch_path = dir.path().join("official.hdiff");
    let out = dir.path().join("out");
    let created = run(&[
        &old,
        &new,
        Path::new("--patch-format"),
        Path::new("hdiff"),
        Path::new("--hdiffz"),
        &hdiffz,
        Path::new("--patch-output"),
        &patch_path,
        Path::new("--no-color"),
    ]);
    assert_eq!(created.status.code(), Some(0), "{}", stderr(&created));
    let applied = run(&[
        &old,
        Path::new("--apply-patch"),
        &patch_path,
        Path::new("--merge-output"),
        &out,
        Path::new("--no-color"),
    ]);
    assert_eq!(applied.status.code(), Some(0), "{}", stderr(&applied));
    assert_eq!(std::fs::read(out.join("keep.txt")).unwrap(), b"new");
    assert_eq!(std::fs::read(out.join("same.txt")).unwrap(), b"same");
    assert_eq!(std::fs::read(out.join("sub/deep.txt")).unwrap(), b"deep");
    assert!(!out.join("remove.txt").exists());

    let zstd_patch = dir.path().join("official-zstd.hdiff");
    let zstd_out = dir.path().join("zstd-out");
    let status = Command::new(hdiffz)
        .args([
            "-f",
            "-c-zstd",
            old.to_str().unwrap(),
            new.to_str().unwrap(),
            zstd_patch.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status.success(), "hdiffz failed to create zstd dir patch");
    let applied = run(&[
        &old,
        Path::new("--apply-patch"),
        &zstd_patch,
        Path::new("--merge-output"),
        &zstd_out,
        Path::new("--no-color"),
    ]);
    assert_eq!(applied.status.code(), Some(0), "{}", stderr(&applied));
    assert_eq!(std::fs::read(zstd_out.join("keep.txt")).unwrap(), b"new");
    assert_eq!(std::fs::read(zstd_out.join("same.txt")).unwrap(), b"same");
}

#[test]
fn applies_pure_rust_uncompressed_hdiff() {
    let dir = tempfile::tempdir().unwrap();
    let output_path = dir.path().join("patched.bin");
    let output = run(&[
        &fixture("binary/old.bin"),
        Path::new("--apply-patch"),
        &fixture("hdiff/change.hdiff"),
        Path::new("--merge-output"),
        &output_path,
        Path::new("--no-color"),
    ]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(stdout(&output).contains("pure Rust"), "{}", stdout(&output));
    assert_eq!(
        std::fs::read(output_path).unwrap(),
        std::fs::read(fixture("binary/new.bin")).unwrap()
    );
}

#[test]
fn applies_pure_rust_zstd_hdiff() {
    let dir = tempfile::tempdir().unwrap();
    let output_path = dir.path().join("patched-large.bin");
    let output = run(&[
        &fixture("hdiff/large-old.bin"),
        Path::new("--apply-patch"),
        &fixture("hdiff/large-zstd.hdiff"),
        Path::new("--merge-output"),
        &output_path,
        Path::new("--no-color"),
    ]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(stdout(&output).contains("pure Rust"), "{}", stdout(&output));
    assert_eq!(
        std::fs::read(output_path).unwrap(),
        std::fs::read(fixture("hdiff/large-new.bin")).unwrap()
    );
}

#[test]
fn applies_pure_rust_lzma2_hdiff() {
    let dir = tempfile::tempdir().unwrap();
    let output_path = dir.path().join("patched-lzma2.bin");
    let output = run(&[
        &fixture("hdiff/large-old.bin"),
        Path::new("--apply-patch"),
        &fixture("hdiff/large-lzma2.hdiff"),
        Path::new("--merge-output"),
        &output_path,
        Path::new("--no-color"),
    ]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(stdout(&output).contains("pure Rust"), "{}", stdout(&output));
    assert_eq!(
        std::fs::read(output_path).unwrap(),
        std::fs::read(fixture("hdiff/large-new.bin")).unwrap()
    );
}

#[test]
fn hdiff_patch_compatible_file_and_folder_roundtrip() {
    let (Some(hdiffz), Some(hpatchz)) = (local_tool("hdiffz"), local_tool("hpatchz")) else {
        eprintln!("skipping HDiffPatch test: hdiffz/hpatchz not downloaded");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let file_patch = dir.path().join("file.hdiff");
    let file_out = dir.path().join("new.bin");
    let created = run(&[
        &fixture("binary/old.bin"),
        &fixture("binary/new.bin"),
        Path::new("--patch-format"),
        Path::new("hdiff"),
        Path::new("--hdiffz"),
        &hdiffz,
        Path::new("--patch-output"),
        &file_patch,
        Path::new("--no-color"),
    ]);
    assert_eq!(created.status.code(), Some(0), "{}", stderr(&created));
    let applied = run(&[
        &fixture("binary/old.bin"),
        Path::new("--patch-format"),
        Path::new("hdiff"),
        Path::new("--hpatchz"),
        &hpatchz,
        Path::new("--apply-patch"),
        &file_patch,
        Path::new("--merge-output"),
        &file_out,
        Path::new("--no-color"),
    ]);
    assert_eq!(applied.status.code(), Some(0), "{}", stderr(&applied));
    assert_eq!(
        std::fs::read(&file_out).unwrap(),
        std::fs::read(fixture("binary/new.bin")).unwrap()
    );

    let folder_patch = dir.path().join("folder.hdiff");
    let folder_out = dir.path().join("folder-out");
    let created = run(&[
        &fixture("dir/left"),
        &fixture("dir/right"),
        Path::new("--patch-format"),
        Path::new("hdiff"),
        Path::new("--hdiffz"),
        &hdiffz,
        Path::new("--patch-output"),
        &folder_patch,
        Path::new("--no-color"),
    ]);
    assert_eq!(created.status.code(), Some(0), "{}", stderr(&created));
    let applied = run(&[
        &fixture("dir/left"),
        Path::new("--patch-format"),
        Path::new("hdiff"),
        Path::new("--hpatchz"),
        &hpatchz,
        Path::new("--apply-patch"),
        &folder_patch,
        Path::new("--merge-output"),
        &folder_out,
        Path::new("--no-color"),
    ]);
    assert_eq!(applied.status.code(), Some(0), "{}", stderr(&applied));
    assert!(folder_out.join("new-only.txt").exists());
    assert!(!folder_out.join("old-only.txt").exists());
}

#[test]
fn folder_patch_create_and_apply() {
    let dir = tempfile::tempdir().unwrap();
    let left = dir.path().join("left");
    let right = dir.path().join("right");
    let output_dir = dir.path().join("applied");
    let patch = dir.path().join("folder.patch");
    for root in [&left, &right] {
        std::fs::create_dir_all(root).unwrap();
    }
    std::fs::create_dir_all(left.join("nested")).unwrap();
    std::fs::create_dir_all(right.join("nested")).unwrap();
    std::fs::write(left.join("same.txt"), "same\n").unwrap();
    std::fs::write(right.join("same.txt"), "same\n").unwrap();
    std::fs::write(left.join("nested/change.txt"), "old\n").unwrap();
    std::fs::write(right.join("nested/change.txt"), "new\n").unwrap();
    std::fs::write(left.join("removed.txt"), "left only\n").unwrap();
    std::fs::write(right.join("added.txt"), "right only\n").unwrap();

    let created = run_str(&[
        left.to_str().unwrap(),
        right.to_str().unwrap(),
        "--patch-output",
        patch.to_str().unwrap(),
        "--no-color",
    ]);
    assert_eq!(created.status.code(), Some(0), "{}", stderr(&created));
    assert!(
        stdout(&created).contains("folder patch created"),
        "{}",
        stdout(&created)
    );

    let applied = run_str(&[
        left.to_str().unwrap(),
        "--apply-patch",
        patch.to_str().unwrap(),
        "--merge-output",
        output_dir.to_str().unwrap(),
        "--no-color",
    ]);
    assert_eq!(applied.status.code(), Some(0), "{}", stderr(&applied));
    assert!(
        stdout(&applied).contains("folder patch applied"),
        "{}",
        stdout(&applied)
    );
    assert_eq!(
        std::fs::read_to_string(output_dir.join("nested/change.txt")).unwrap(),
        "new\n"
    );
    assert_eq!(
        std::fs::read_to_string(output_dir.join("added.txt")).unwrap(),
        "right only\n"
    );
    assert!(!output_dir.join("removed.txt").exists());
    assert!(
        left.join("removed.txt").exists(),
        "source must stay unchanged"
    );
}

#[test]
fn external_tool_delegates_and_propagates_status() {
    let script = "import sys; print('external:'); sys.exit(1)";
    let output = run_str(&[
        fixture("text/old.txt").to_str().unwrap(),
        fixture("text/new.txt").to_str().unwrap(),
        "--external-tool",
        "python",
        "--external-arg",
        "-c",
        "--external-arg",
        script,
    ]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(stdout(&output).contains("external:"), "{}", stdout(&output));

    let equal = run_str(&[
        fixture("text/old.txt").to_str().unwrap(),
        fixture("text/old.txt").to_str().unwrap(),
        "--external-tool",
        "python",
        "--external-arg",
        "-c",
        "--external-arg",
        "print('same')",
    ]);
    assert_eq!(equal.status.code(), Some(0), "{}", stderr(&equal));
    assert!(stdout(&equal).contains("same"), "{}", stdout(&equal));

    let failed = run_str(&[
        fixture("text/old.txt").to_str().unwrap(),
        fixture("text/new.txt").to_str().unwrap(),
        "--external-tool",
        "python",
        "--external-arg",
        "-c",
        "--external-arg",
        "import sys; sys.exit(3)",
    ]);
    assert_eq!(failed.status.code(), Some(2), "{}", stderr(&failed));
    assert!(
        stderr(&failed).contains("external tool"),
        "{}",
        stderr(&failed)
    );
}

#[test]
fn binary_patch_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let patch = dir.path().join("change.patch");
    let output = run(&[
        &fixture("binary/old.bin"),
        &fixture("binary/new.bin"),
        Path::new("--patch-output"),
        &patch,
    ]);
    assert_eq!(output.status.code(), Some(0));
    let bytes = std::fs::read(&patch).unwrap();
    assert!(bytes.len() > 20, "patch is unexpectedly small");
    assert!(bytes.iter().any(|byte| *byte != 0), "patch is empty");
}

#[test]
fn alternate_reports_are_well_formed() {
    let markdown = run_str(&[
        fixture("json/old.json").to_str().unwrap(),
        fixture("json/new.json").to_str().unwrap(),
        "--format",
        "markdown",
    ]);
    let text = stdout(&markdown);
    assert!(text.starts_with("# DiffEverything report"));
    assert!(text.contains("JSON changes"));

    let html = run_str(&[
        fixture("dir/left").to_str().unwrap(),
        fixture("dir/right").to_str().unwrap(),
        "--format",
        "html",
    ]);
    let text = stdout(&html);
    assert!(text.starts_with("<!doctype html>"));
    assert!(text.trim_end().ends_with("</html>"));

    let unified = run_str(&[
        fixture("text/old.txt").to_str().unwrap(),
        fixture("text/new.txt").to_str().unwrap(),
        "--format",
        "unified",
        "--detail",
        "full",
    ]);
    let text = stdout(&unified);
    assert!(text.starts_with("--- "));
    assert!(text.contains("+++ "));
    assert!(text.contains("@@ -1,5 +1,7 @@"));
}

fn workspace_path(relative: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.join(relative)
}

fn local_tool(name: &str) -> Option<PathBuf> {
    let mut workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    workspace.pop();
    workspace.pop();
    for relative in [
        "testdata/external/tesseract",
        "testdata/external/poppler",
        "testdata/external/hdiffpatch",
        "testdata/tesseract",
        "testdata/poppler",
    ] {
        let root = workspace.join(relative);
        if !root.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&root)
            .max_depth(7)
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

#[test]
fn tree_mode_finds_modified_node() {
    let output = run_str(&[
        fixture("code/old.rs").to_str().unwrap(),
        fixture("code/new.rs").to_str().unwrap(),
        "--mode",
        "tree",
        "--ignore-comments",
        "--no-color",
    ]);
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(text.contains("rust tree: +0 -0 ~1"), "{text}");
    assert!(text.contains("let_declaration"), "{text}");
}

#[test]
fn folder_sync_plan_and_apply() {
    let dir = tempfile::tempdir().unwrap();
    let left = dir.path().join("left");
    let right = dir.path().join("right");
    std::fs::create_dir_all(&left).unwrap();
    std::fs::create_dir_all(&right).unwrap();
    std::fs::write(left.join("a.txt"), "new a").unwrap();
    std::fs::write(left.join("new.txt"), "new").unwrap();
    std::fs::write(right.join("a.txt"), "old a").unwrap();
    std::fs::write(right.join("extra.txt"), "extra").unwrap();

    let plan = run_str(&[
        left.to_str().unwrap(),
        right.to_str().unwrap(),
        "--sync",
        "mirror-left-to-right",
    ]);
    assert_eq!(plan.status.code(), Some(1));
    let text = stdout(&plan);
    assert!(text.contains("COPY   a.txt"));
    assert!(text.contains("DELETE extra.txt"));

    let applied = run_str(&[
        left.to_str().unwrap(),
        right.to_str().unwrap(),
        "--sync",
        "mirror-left-to-right",
        "--sync-apply",
        "--backup-dir",
        dir.path().join("backup").to_str().unwrap(),
    ]);
    assert_eq!(applied.status.code(), Some(0), "{}", stderr(&applied));
    assert_eq!(
        std::fs::read_to_string(right.join("a.txt")).unwrap(),
        "new a"
    );
    assert!(right.join("new.txt").exists());
    assert!(!right.join("extra.txt").exists());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("backup/extra.txt")).unwrap(),
        "extra"
    );
}

#[test]
fn sync_apply_requires_sync_mode() {
    let output = run_str(&[
        fixture("dir/left").to_str().unwrap(),
        fixture("dir/right").to_str().unwrap(),
        "--sync-apply",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("--sync-apply requires --sync"));
}

#[test]
fn notebook_cell_merge() {
    use serde_json::json;

    fn notebook(cells: Value) -> Value {
        json!({
            "cells": cells,
            "metadata": {},
            "nbformat": 4,
            "nbformat_minor": 5
        })
    }

    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("base.ipynb");
    let ours = dir.path().join("ours.ipynb");
    let theirs = dir.path().join("theirs.ipynb");
    let conflict = dir.path().join("conflict.ipynb");

    std::fs::write(
        &base,
        serde_json::to_vec_pretty(&notebook(json!([
            {"id":"a","cell_type":"markdown","metadata":{},"source":["# Base\n"]},
            {"id":"b","cell_type":"code","metadata":{},"execution_count":null,"outputs":[],"source":["x = 1\n"]}
        ])))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(
        &ours,
        serde_json::to_vec_pretty(&notebook(json!([
            {"id":"a","cell_type":"markdown","metadata":{},"source":["# Ours\n"]},
            {"id":"b","cell_type":"code","metadata":{},"execution_count":null,"outputs":[],"source":["x = 1\n"]}
        ])))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(
        &theirs,
        serde_json::to_vec_pretty(&notebook(json!([
            {"id":"a","cell_type":"markdown","metadata":{},"source":["# Base\n"]},
            {"id":"b","cell_type":"code","metadata":{},"execution_count":null,"outputs":[],"source":["x = 2\n"]}
        ])))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(
        &conflict,
        serde_json::to_vec_pretty(&notebook(json!([
            {"id":"a","cell_type":"markdown","metadata":{},"source":["# Theirs\n"]},
            {"id":"b","cell_type":"code","metadata":{},"execution_count":null,"outputs":[],"source":["x = 1\n"]}
        ])))
        .unwrap(),
    )
    .unwrap();

    let clean = run_str(&[
        ours.to_str().unwrap(),
        theirs.to_str().unwrap(),
        "--base",
        base.to_str().unwrap(),
    ]);
    assert_eq!(clean.status.code(), Some(0), "{}", stderr(&clean));
    let merged: Value = serde_json::from_slice(&clean.stdout).unwrap();
    let cells = merged["cells"].as_array().unwrap();
    assert_eq!(cells.len(), 2);
    assert!(cells[0]["source"].to_string().contains("Ours"));
    assert!(cells[1]["source"].to_string().contains("x = 2"));

    let conflicted = run_str(&[
        ours.to_str().unwrap(),
        conflict.to_str().unwrap(),
        "--base",
        base.to_str().unwrap(),
    ]);
    assert_eq!(conflicted.status.code(), Some(1));
    let text = stdout(&conflicted);
    assert!(text.contains("<<<<<<< ours"), "{text}");
    assert!(text.contains(">>>>>>> theirs"), "{text}");
}

#[test]
fn session_save_and_load() {
    let dir = tempfile::tempdir().unwrap();
    let session = dir.path().join("session.json");
    let save = run_str(&[
        fixture("json/old.json").to_str().unwrap(),
        fixture("json/new.json").to_str().unwrap(),
        "--save-session",
        session.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(save.status.code(), Some(1), "{}", stderr(&save));
    let session_value: Value = serde_json::from_slice(&std::fs::read(&session).unwrap()).unwrap();
    assert_eq!(session_value["version"], 1);
    assert!(session_value["left"].as_str().unwrap().contains("old.json"));

    let load = run_str(&[
        "--load-session",
        session.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(load.status.code(), Some(1), "{}", stderr(&load));
    let report: Value = serde_json::from_slice(&load.stdout).unwrap();
    assert_eq!(report["status"], "different");
    assert_eq!(report["summary"]["structured_changes"], 5);

    let ambiguous = run_str(&[
        fixture("json/old.json").to_str().unwrap(),
        fixture("json/new.json").to_str().unwrap(),
        "--load-session",
        session.to_str().unwrap(),
    ]);
    assert_eq!(ambiguous.status.code(), Some(2));
    assert!(stderr(&ambiguous).contains("do not pass LEFT/RIGHT"));
}

#[test]
fn ocr_images_when_tesseract_is_available() {
    if local_tool("tesseract").is_none() {
        eprintln!("skipping OCR test: tesseract not downloaded");
        return;
    }
    let tesseract = local_tool("tesseract").unwrap();
    let tessdata = workspace_path("testdata/external/tesseract/tessdata");
    let output = run_str(&[
        fixture("ocr/old.png").to_str().unwrap(),
        fixture("ocr/new.png").to_str().unwrap(),
        "--ocr",
        "--tesseract",
        tesseract.to_str().unwrap(),
        "--tessdata",
        tessdata.to_str().unwrap(),
        "--no-color",
    ]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("ocr:"), "{text}");
    assert!(text.contains("Hello world"), "{text}");
    assert!(text.contains("Hello OCR"), "{text}");
}

#[test]
fn pdf_ocr_when_tools_are_available() {
    if local_tool("tesseract").is_none() || local_tool("pdftoppm").is_none() {
        eprintln!("skipping PDF OCR test: tesseract/pdftoppm not downloaded");
        return;
    }
    let tesseract = local_tool("tesseract").unwrap();
    let pdftoppm = local_tool("pdftoppm").unwrap();
    let tessdata = workspace_path("testdata/external/tesseract/tessdata");
    let output = run_str(&[
        fixture("pdf/old.pdf").to_str().unwrap(),
        fixture("pdf/new.pdf").to_str().unwrap(),
        "--ocr",
        "--tesseract",
        tesseract.to_str().unwrap(),
        "--tessdata",
        tessdata.to_str().unwrap(),
        "--pdftoppm",
        pdftoppm.to_str().unwrap(),
        "--pdf-max-pages",
        "1",
        "--no-color",
        "--detail",
        "full",
    ]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("pdf-ocr:"), "{text}");
    assert!(text.contains("# Page 1"), "{text}");
}

#[test]
fn pdf_visual_diff_when_pdftoppm_is_available() {
    if local_tool("pdftoppm").is_none() {
        eprintln!("skipping PDF visual test: pdftoppm not downloaded");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let output_dir = dir.path().join("pdf-visual");
    let pdftoppm = local_tool("pdftoppm").unwrap();
    let output = run_str(&[
        fixture("pdf/old.pdf").to_str().unwrap(),
        fixture("pdf/new.pdf").to_str().unwrap(),
        "--pdf-visual",
        "--pdftoppm",
        pdftoppm.to_str().unwrap(),
        "--pdf-output-dir",
        output_dir.to_str().unwrap(),
        "--pdf-dpi",
        "40",
        "--no-color",
    ]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("PDF visual diff"), "{text}");
    assert!(text.contains("page 1:"), "{text}");
    assert!(output_dir.join("report.html").exists());
    assert!(output_dir.join("heatmaps/page-1.png").exists());
    let html = std::fs::read_to_string(output_dir.join("report.html")).unwrap();
    assert!(
        html.contains("data:image/png;base64,"),
        "{}",
        &html[..html.len().min(300)]
    );
    assert!(
        !html.contains("src=\"heatmaps/"),
        "report.html should embed images"
    );
    let pdf = std::fs::read(output_dir.join("report.pdf")).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    assert!(pdf.len() > 1000);
    let overlay = std::fs::read(output_dir.join("report-overlay.pdf")).unwrap();
    assert!(overlay.starts_with(b"%PDF"));
    assert!(output_dir.join("overlays/page-1.png").exists());

    let vector_path = output_dir.join("report-vector.pdf");
    let vector = std::fs::read(&vector_path).unwrap();
    assert!(vector.starts_with(b"%PDF"));
    let render_dir = dir.path().join("vector-render");
    std::fs::create_dir_all(&render_dir).unwrap();
    let status = Command::new(&pdftoppm)
        .args([
            "-png",
            "-r",
            "30",
            "-f",
            "1",
            "-l",
            "1",
            vector_path.to_str().unwrap(),
            render_dir.join("page").to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status.success());
    let rendered = std::fs::read_dir(&render_dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .find(|path| path.extension().and_then(|e| e.to_str()) == Some("png"))
        .unwrap();
    let image = image::open(rendered).unwrap().to_rgba8();
    let red = image
        .pixels()
        .any(|pixel| pixel.0[0] > 180 && pixel.0[1] < 120 && pixel.0[2] < 120);
    assert!(red, "vector PDF should contain red overlay rectangles");
}

#[test]
fn tui_format_falls_back_when_piped() {
    let output = run_str(&[
        fixture("text/old.txt").to_str().unwrap(),
        fixture("text/new.txt").to_str().unwrap(),
        "--format",
        "tui",
        "--no-color",
    ]);
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(text.contains("DiffEverything"), "{text}");
    assert!(text.contains("@@ "), "{text}");
    assert!(!text.contains('\x1b'), "TUI fallback leaked ANSI escapes");
}

#[test]
fn sftp_known_hosts_verification() {
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let script = workspace_path("scripts/sftp-test-server.py");
    let root = workspace_path("fixtures/text");
    let temp = tempfile::tempdir().unwrap();
    let host_key_out = temp.path().join("host-key.txt");
    struct KillOnDrop(std::process::Child);
    impl Drop for KillOnDrop {
        fn drop(&mut self) {
            let _ = self.0.kill();
        }
    }

    let _child = match Command::new("python")
        .args([
            script.to_str().unwrap(),
            "--root",
            root.to_str().unwrap(),
            "--port",
            &port.to_string(),
            "--user",
            "test",
            "--password",
            "test",
            "--host-key-out",
            host_key_out.to_str().unwrap(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => KillOnDrop(child),
        Err(_) => return,
    };
    let mut ready = false;
    for _ in 0..50 {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() && host_key_out.is_file() {
            ready = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if !ready {
        return;
    }

    let public_key = std::fs::read_to_string(&host_key_out).unwrap();
    let public_key = public_key.trim();
    let known_hosts = temp.path().join("known_hosts");
    std::fs::write(&known_hosts, format!("[127.0.0.1]:{port} {public_key}\n")).unwrap();

    let old = format!("sftp://test:test@127.0.0.1:{port}/old.txt");
    let new = format!("sftp://test:test@127.0.0.1:{port}/new.txt");
    let output = run_str(&[
        old.as_str(),
        new.as_str(),
        "--sftp-known-hosts",
        known_hosts.to_str().unwrap(),
        "--no-color",
    ]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));

    // A known_hosts entry for the wrong host must refuse the connection.
    std::fs::write(&known_hosts, format!("[127.0.0.1]:1 {public_key}")).unwrap();
    let output = run_str(&[
        old.as_str(),
        new.as_str(),
        "--sftp-known-hosts",
        known_hosts.to_str().unwrap(),
        "--no-color",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("not present"),
        "{}",
        stderr(&output)
    );

    // A tampered key for the same host must also be refused.
    let mut tampered = public_key.trim_end().to_string();
    let last = tampered.pop().unwrap();
    tampered.push(if last == 'A' { 'B' } else { 'A' });
    std::fs::write(&known_hosts, format!("[127.0.0.1]:{port} {tampered}\n")).unwrap();
    let output = run_str(&[
        old.as_str(),
        new.as_str(),
        "--sftp-known-hosts",
        known_hosts.to_str().unwrap(),
        "--no-color",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("mismatch"), "{}", stderr(&output));
}

#[test]
fn webdav_remote_inputs_with_local_server() {
    use tiny_http::{Response, Server};

    let server = Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let text_dir = workspace_path("fixtures/text");
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let name = request.url().trim_start_matches('/');
            let path = text_dir.join(name);
            match std::fs::read(path) {
                Ok(bytes) => {
                    let _ = request.respond(Response::from_data(bytes));
                }
                Err(_) => {
                    let _ = request.respond(Response::empty(404));
                }
            }
        }
    });

    let old = format!("webdav://127.0.0.1:{port}/old.txt");
    let new = format!("http://127.0.0.1:{port}/new.txt");
    let output = run_str(&[old.as_str(), new.as_str(), "--no-color"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("webdav://127.0.0.1"), "{text}");
    assert!(text.contains("@@ "), "{text}");
}

#[test]
fn webdav_uses_basic_auth() {
    use tiny_http::{Response, Server};

    let server = Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let text_dir = workspace_path("fixtures/text");
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let authorized = request.headers().iter().any(|header| {
                header.field.equiv("Authorization") && header.value.as_str() == "Basic dXNlcjpwYXNz"
            });
            if !authorized {
                let _ = request.respond(Response::empty(401));
                continue;
            }
            let name = request.url().trim_start_matches('/');
            match std::fs::read(text_dir.join(name)) {
                Ok(bytes) => {
                    let _ = request.respond(Response::from_data(bytes));
                }
                Err(_) => {
                    let _ = request.respond(Response::empty(404));
                }
            }
        }
    });

    let old = format!("webdav://127.0.0.1:{port}/old.txt");
    let new = format!("http://127.0.0.1:{port}/new.txt");
    let output = run_str(&[
        old.as_str(),
        new.as_str(),
        "--remote-user",
        "user",
        "--remote-password",
        "pass",
        "--no-color",
    ]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(stdout(&output).contains("@@ "), "{}", stdout(&output));
}

#[test]
fn remote_inputs_with_same_file_name_do_not_collide() {
    use tiny_http::{Response, Server};

    let server = Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let dir = workspace_path("fixtures/dir");
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let name = request.url().trim_start_matches('/');
            match std::fs::read(dir.join(name)) {
                Ok(bytes) => {
                    let _ = request.respond(Response::from_data(bytes));
                }
                Err(_) => {
                    let _ = request.respond(Response::empty(404));
                }
            }
        }
    });

    let left = format!("webdav://127.0.0.1:{port}/left/README.md");
    let right = format!("webdav://127.0.0.1:{port}/right/README.md");
    let output = run_str(&[left.as_str(), right.as_str(), "--no-color"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("added in the right side"), "{text}");
}

#[test]
fn webdav_directory_recursion_with_local_server() {
    use tiny_http::{Response, Server};

    let server = Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let dir = workspace_path("fixtures/dir");
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let url_path = request.url().trim_start_matches('/').to_string();
            if request.method().as_str() == "PROPFIND" {
                let rel = url_path.trim_end_matches('/').to_string();
                let root = dir.join(&rel);
                if !root.is_dir() {
                    let _ = request.respond(Response::empty(404));
                    continue;
                }
                let mut body =
                    String::from("<?xml version=\"1.0\"?><d:multistatus xmlns:d=\"DAV:\">");
                body.push_str(&format!(
                    "<d:response><d:href>/{rel}/</d:href><d:propstat><d:prop>\
                     <d:resourcetype><d:collection/></d:resourcetype></d:prop></d:propstat>\
                     </d:response>"
                ));
                for entry in std::fs::read_dir(&root).unwrap() {
                    let entry = entry.unwrap();
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let is_dir = entry.file_type().unwrap().is_dir();
                    let suffix = if is_dir { "/" } else { "" };
                    let collection = if is_dir { "<d:collection/>" } else { "" };
                    body.push_str(&format!(
                        "<d:response><d:href>/{rel}/{name}{suffix}</d:href><d:propstat><d:prop>\
                         <d:resourcetype>{collection}</d:resourcetype></d:prop></d:propstat>\
                         </d:response>"
                    ));
                }
                body.push_str("</d:multistatus>");
                let response = Response::from_string(body)
                    .with_header(
                        tiny_http::Header::from_bytes("Content-Type", "application/xml").unwrap(),
                    )
                    .with_status_code(207);
                let _ = request.respond(response);
            } else {
                match std::fs::read(dir.join(&url_path)) {
                    Ok(bytes) => {
                        let _ = request.respond(Response::from_data(bytes));
                    }
                    Err(_) => {
                        let _ = request.respond(Response::empty(404));
                    }
                }
            }
        }
    });

    let left = format!("webdav://127.0.0.1:{port}/left");
    let right = format!("webdav://127.0.0.1:{port}/right");
    let output = run_str(&[left.as_str(), right.as_str(), "--no-color"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("directory:"), "{text}");
    assert!(text.contains("old-only.txt"), "{text}");
    assert!(text.contains("new-only.txt"), "{text}");
}

#[test]
fn sftp_remote_inputs_with_local_server() {
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let script = workspace_path("scripts/sftp-test-server.py");
    let root = workspace_path("fixtures/text");
    let mut child = match Command::new("python")
        .args([
            script.to_str().unwrap(),
            "--root",
            root.to_str().unwrap(),
            "--port",
            &port.to_string(),
            "--user",
            "test",
            "--password",
            "test",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            eprintln!("skipping SFTP test: python unavailable ({error})");
            return;
        }
    };

    let mut ready = false;
    for _ in 0..50 {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            ready = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if !ready {
        let _ = child.kill();
        eprintln!("skipping SFTP test: local server did not start (asyncssh missing?)");
        return;
    }

    let old = format!("sftp://test:test@127.0.0.1:{port}/old.txt");
    let new = format!("sftp://test:test@127.0.0.1:{port}/new.txt");
    let output = run_str(&[old.as_str(), new.as_str(), "--no-color"]);
    let _ = child.kill();
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("sftp://test:test@127.0.0.1"), "{text}");
    assert!(text.contains("@@ "), "{text}");
}

#[test]
fn smb_remote_inputs_with_local_server() {
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let script = workspace_path("scripts/smb-test-server.py");
    let root = workspace_path("fixtures/dir");
    let mut child = match Command::new("python")
        .args([
            script.to_str().unwrap(),
            "--root",
            root.to_str().unwrap(),
            "--port",
            &port.to_string(),
            "--user",
            "test",
            "--password",
            "test",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            eprintln!("skipping SMB test: python unavailable ({error})");
            return;
        }
    };

    let mut ready = false;
    for _ in 0..50 {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            ready = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if !ready {
        let _ = child.kill();
        eprintln!("skipping SMB test: local server did not start (impacket missing?)");
        return;
    }

    let old = format!("smb://test:test@127.0.0.1:{port}/data/left/src/main.rs");
    let new = format!("smb://test:test@127.0.0.1:{port}/data/right/src/main.rs");
    let file_output = run_str(&[old.as_str(), new.as_str(), "--no-color"]);
    let left = format!("smb://test:test@127.0.0.1:{port}/data/left");
    let right = format!("smb://test:test@127.0.0.1:{port}/data/right");
    let dir_output = run_str(&[left.as_str(), right.as_str(), "--no-color"]);
    let _ = child.kill();

    assert_eq!(
        file_output.status.code(),
        Some(1),
        "{}",
        stderr(&file_output)
    );
    let text = stdout(&file_output);
    assert!(text.contains("smb://test:test@127.0.0.1"), "{text}");
    assert!(text.contains("@@ "), "{text}");

    assert_eq!(dir_output.status.code(), Some(1), "{}", stderr(&dir_output));
    let text = stdout(&dir_output);
    assert!(text.contains("directory:"), "{text}");
    assert!(text.contains("old-only.txt"), "{text}");
    assert!(text.contains("new-only.txt"), "{text}");
}

#[test]
fn sftp_remote_directories_with_local_server() {
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let script = workspace_path("scripts/sftp-test-server.py");
    let root = workspace_path("fixtures/dir");
    let mut child = match Command::new("python")
        .args([
            script.to_str().unwrap(),
            "--root",
            root.to_str().unwrap(),
            "--port",
            &port.to_string(),
            "--user",
            "test",
            "--password",
            "test",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return,
    };
    let mut ready = false;
    for _ in 0..50 {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            ready = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if !ready {
        let _ = child.kill();
        return;
    }

    let left = format!("sftp://test:test@127.0.0.1:{port}/left");
    let right = format!("sftp://test:test@127.0.0.1:{port}/right");
    let output = run_str(&[left.as_str(), right.as_str(), "--no-color"]);
    let _ = child.kill();
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("directory:"), "{text}");
    assert!(text.contains("old-only.txt"), "{text}");
    assert!(text.contains("new-only.txt"), "{text}");
}

#[test]
fn html_report_escapes_untrusted_text() {
    let dir = tempfile::tempdir().unwrap();
    let left = dir.path().join("left.txt");
    let right = dir.path().join("right.txt");
    std::fs::write(&left, "<script>alert(1)</script>\n").unwrap();
    std::fs::write(&right, "<script>alert(2)</script>\n").unwrap();

    let output = run_str(&[
        left.to_str().unwrap(),
        right.to_str().unwrap(),
        "--format",
        "html",
    ]);
    let text = stdout(&output);
    assert!(text.contains("&lt;script&gt;"));
    assert!(!text.contains("<script>alert"));
}

#[test]
fn piped_output_has_no_ansi_escapes() {
    let output = run_str(&[
        fixture("text/old.txt").to_str().unwrap(),
        fixture("text/new.txt").to_str().unwrap(),
    ]);
    let text = stdout(&output);
    assert!(
        !text.contains('\x1b'),
        "color escapes leaked into piped output"
    );
}
