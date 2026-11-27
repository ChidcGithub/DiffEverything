use anyhow::{Context, Result};
use base64::Engine as _;
use diff_core::model::{
    ArtifactInfo, DiffBody, DiffNode, DiffReport, DiffStatus, FileKind, Summary, TOOL_NAME,
    TOOL_VERSION,
};
use diff_core::progress::ProgressHandle;
use serde_json::{Map, Value, json};
use std::fs;
use std::io::{IsTerminal, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use url::Url;

#[cfg(windows)]
const ARIA2_BYTES: &[u8] = include_bytes!("../assets/aria2c.exe");
#[cfg(windows)]
const ARIA2_VERSION: &str = "1.37.0";
const LARGE_FILE_BYTES: u64 = 100 * 1024 * 1024;
const HISTORY_LIMIT: usize = 2000;

#[derive(Clone, Copy)]
pub struct DownloadConfig<'a> {
    pub yes: bool,
    pub aria2: Option<&'a Path>,
    pub timeout: Duration,
    pub user: Option<&'a str>,
    pub password: Option<&'a str>,
    pub progress: &'a ProgressHandle,
}

/// Return true for HTTP(S) file links. Git repository URLs are handled by the
/// git-history path instead of the plain download path.
pub fn is_http_link(spec: &str) -> bool {
    if is_git_url(spec) {
        return false;
    }
    Url::parse(spec)
        .ok()
        .is_some_and(|url| matches!(url.scheme(), "http" | "https"))
}

pub fn is_git_url(spec: &str) -> bool {
    if spec.starts_with("git@") || spec.starts_with("git://") || spec.starts_with("ssh://") {
        return true;
    }
    if spec.starts_with("http://") || spec.starts_with("https://") || spec.starts_with("file://") {
        return spec
            .split(['?', '#'])
            .next()
            .is_some_and(|value| value.trim_end_matches('/').ends_with(".git"));
    }
    false
}

pub fn file_name(spec: &str) -> String {
    let raw = Url::parse(spec)
        .ok()
        .and_then(|url| {
            url.path_segments()
                .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
                .map(str::to_string)
        })
        .unwrap_or_default();
    sanitize_file_name(&raw)
}

fn sanitize_file_name(raw: &str) -> String {
    let name: String = raw
        .chars()
        .map(|character| match character {
            '%' | '\\' | '/' | ':' => '_',
            character if character.is_control() => '_',
            character => character,
        })
        .collect();
    let upper_stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = matches!(upper_stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper_stem.len() == 4
            && (upper_stem.starts_with("COM") || upper_stem.starts_with("LPT"))
            && upper_stem[3..].parse::<u8>().is_ok());
    if name.is_empty()
        || name.chars().all(|character| matches!(character, '.' | ' '))
        || name.ends_with('.')
        || name.ends_with(' ')
        || reserved
    {
        return "download.bin".to_string();
    }
    if name.chars().count() <= 120 {
        return name;
    }
    let extension = Path::new(&name)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!(".{extension}"))
        .unwrap_or_default();
    let keep = 120usize.saturating_sub(extension.chars().count());
    let stem: String = name.chars().take(keep).collect();
    format!("{stem}{extension}")
}

pub fn resolve_http_input(
    spec: &str,
    temp_dir: &Path,
    config: DownloadConfig<'_>,
) -> Result<PathBuf> {
    // Give each link its own directory so two URLs with the same final path
    // segment cannot overwrite each other in the shared temp directory.
    let root = temp_dir.join(format!("link-{:016x}", unique_suffix(spec)));
    fs::create_dir_all(&root)?;
    let destination = root.join(file_name(spec));
    download_http(spec, &destination, config)?;
    Ok(destination)
}

pub fn resolve_repository(
    spec: &str,
    temp_dir: &Path,
    progress: &ProgressHandle,
    timeout: Duration,
) -> Result<PathBuf> {
    if Path::new(spec).is_dir() {
        return prepare_repository(spec, temp_dir, progress, timeout);
    }
    let destination = temp_dir.join(format!("repo-{:016x}", unique_suffix(spec)));
    prepare_repository(spec, &destination, progress, timeout)
}

fn unique_suffix(spec: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    spec.hash(&mut hasher);
    hasher.finish()
}

pub fn download_http(spec: &str, destination: &Path, config: DownloadConfig<'_>) -> Result<()> {
    let url = Url::parse(spec).with_context(|| format!("invalid URL {spec}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        anyhow::bail!("unsupported link scheme {}", url.scheme());
    }
    let (clean_url, authorization) = split_credentials(&url, config.user, config.password);
    let total = probe_content_length(&clean_url, authorization.as_deref());
    if let Some(total) = total.filter(|total| *total > LARGE_FILE_BYTES) {
        eprintln!(
            "warning: {} is {} (limit 100 MiB); use -y to allow this download",
            clean_url,
            display_size(total)
        );
    }
    check_size_limit(total, config.yes)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let aria2 = match aria2_executable(config.aria2) {
        Ok(aria2) => aria2,
        Err(error) if config.aria2.is_some() => return Err(error),
        Err(error) => {
            eprintln!("warning: {error}; using the built-in HTTP downloader");
            None
        }
    };
    if let Some(aria2) = aria2 {
        let name = destination
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "download.bin".to_string());
        config.progress.add_total(1);
        config.progress.set_message(&format!("Downloading {name}"));
        let mut command = Command::new(&aria2);
        command
            .arg("--allow-overwrite=true")
            .arg("--auto-file-renaming=false")
            .arg("--file-allocation=none")
            .arg("--max-connection-per-server=4")
            .arg("--split=4")
            .arg("--min-split-size=1M")
            .arg("--max-tries=2")
            .arg("--retry-wait=1")
            .arg("--connect-timeout=20")
            .arg("--timeout=30")
            .arg("--console-log-level=error")
            .arg("--summary-interval=0")
            .arg("--show-console-readout=false")
            .arg("--download-result=hide")
            .arg(format!(
                "--dir={}",
                destination.parent().unwrap_or(Path::new(".")).display()
            ))
            .arg(format!("--out={name}"));
        if let Some((user, password)) = credentials(&url, config.user, config.password) {
            command.arg(format!("--http-user={user}"));
            command.arg(format!("--http-passwd={password}"));
        }
        command
            .arg(clean_url.as_str())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        match run_with_progress(
            command,
            config.timeout,
            config.progress,
            destination,
            &name,
            "Downloading",
        ) {
            Ok(status) if status.success() && destination.is_file() => {
                warn_if_actual_large(destination, &clean_url, total);
                config.progress.inc(&format!("Downloaded {name}"));
                return Ok(());
            }
            Ok(status) => eprintln!(
                "warning: aria2 exited with {}; falling back to built-in HTTP download",
                status.code().unwrap_or(-1)
            ),
            Err(error) => {
                eprintln!("warning: aria2 failed ({error}); falling back to built-in HTTP download")
            }
        }
    }
    if let Err(error) = download_builtin(
        &clean_url,
        destination,
        authorization.as_deref(),
        config.timeout,
        config.progress,
    ) {
        let _ = fs::remove_file(destination);
        return Err(error);
    }
    warn_if_actual_large(destination, &clean_url, total);
    config
        .progress
        .inc(&format!("Downloaded {}", destination.display()));
    Ok(())
}

fn warn_if_actual_large(destination: &Path, url: &Url, expected: Option<u64>) {
    if expected.is_some_and(|total| total > LARGE_FILE_BYTES) {
        return;
    }
    if let Ok(metadata) = fs::metadata(destination)
        && metadata.len() > LARGE_FILE_BYTES
    {
        eprintln!(
            "warning: downloaded {} is {} (limit 100 MiB)",
            url,
            display_size(metadata.len())
        );
    }
}

fn display_size(bytes: u64) -> String {
    format!("{:.1} MiB", bytes as f64 / 1024.0 / 1024.0)
}

fn check_size_limit(total: Option<u64>, yes: bool) -> Result<()> {
    let Some(total) = total else {
        return Ok(());
    };
    if total <= LARGE_FILE_BYTES || yes {
        return Ok(());
    }
    if !std::io::stdin().is_terminal() {
        anyhow::bail!("download exceeds 100 MiB; rerun with -y to allow it");
    }
    eprint!("Download this file anyway? [y/N]: ");
    std::io::stderr().flush().ok();
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    if matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        Ok(())
    } else {
        anyhow::bail!("download skipped by user")
    }
}

fn probe_content_length(url: &Url, authorization: Option<&str>) -> Option<u64> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(20))
        .build();
    let mut request = agent.head(url.as_str());
    if let Some(authorization) = authorization {
        request = request.set("Authorization", authorization);
    }
    if let Some(total) = request.call().ok().and_then(|response| {
        response
            .header("content-length")
            .and_then(|value| value.parse::<u64>().ok())
    }) && total > 0
    {
        return Some(total);
    }

    // Some servers reject HEAD or answer with a chunked response. A one-byte
    // range request reveals the full size through Content-Range in that case.
    let mut request = agent.get(url.as_str()).set("Range", "bytes=0-0");
    if let Some(authorization) = authorization {
        request = request.set("Authorization", authorization);
    }
    let response = request.call().ok()?;
    if let Some(total) = response
        .header("content-range")
        .and_then(parse_content_range)
    {
        return Some(total);
    }
    response
        .header("content-length")
        .and_then(|value| value.parse().ok())
}

fn parse_content_range(value: &str) -> Option<u64> {
    value.rsplit('/').next()?.trim().parse().ok()
}

fn download_builtin(
    url: &Url,
    destination: &Path,
    authorization: Option<&str>,
    timeout: Duration,
    progress: &ProgressHandle,
) -> Result<()> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(timeout)
        .timeout_write(timeout)
        .build();
    let mut request = agent.get(url.as_str());
    if let Some(authorization) = authorization {
        request = request.set("Authorization", authorization);
    }
    let response = request
        .call()
        .with_context(|| format!("HTTP GET failed for {url}"))?;
    let mut reader = response.into_reader();
    let mut file = fs::File::create(destination)?;
    let mut buffer = [0u8; 64 * 1024];
    let mut written = 0u64;
    let name = destination
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])?;
        written += read as u64;
        progress.set_message(&format!("Downloading {name} ({})", display_size(written)));
    }
    file.flush()?;
    Ok(())
}

fn split_credentials(
    url: &Url,
    fallback_user: Option<&str>,
    fallback_password: Option<&str>,
) -> (Url, Option<String>) {
    let mut clean = url.clone();
    let credentials = credentials(url, fallback_user, fallback_password);
    if !url.username().is_empty() {
        clean.set_username("").ok();
        clean.set_password(None).ok();
    }
    let authorization = credentials.as_ref().map(|(user, password)| {
        let credentials = format!("{user}:{password}");
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(credentials)
        )
    });
    (clean, authorization)
}

fn credentials(
    url: &Url,
    fallback_user: Option<&str>,
    fallback_password: Option<&str>,
) -> Option<(String, String)> {
    if !url.username().is_empty() {
        return Some((
            url.username().to_string(),
            url.password().unwrap_or_default().to_string(),
        ));
    }
    let user = fallback_user.filter(|user| !user.is_empty())?;
    Some((
        user.to_string(),
        fallback_password.unwrap_or_default().to_string(),
    ))
}

fn aria2_executable(explicit: Option<&Path>) -> Result<Option<PathBuf>> {
    if let Some(path) = explicit {
        if !path.is_file() {
            anyhow::bail!("aria2 executable not found at {}", path.display());
        }
        return Ok(Some(path.to_path_buf()));
    }
    #[cfg(windows)]
    {
        let directory = std::env::temp_dir().join("diff-everything-tools");
        fs::create_dir_all(&directory)?;
        // A per-process name avoids a sharing race when several comparisons
        // start at the same time and extract the embedded binary.
        let path = directory.join(format!("aria2c-{ARIA2_VERSION}-{}.exe", std::process::id()));
        if !path.is_file()
            || fs::metadata(&path)
                .map(|metadata| metadata.len())
                .unwrap_or(0)
                != ARIA2_BYTES.len() as u64
        {
            fs::write(&path, ARIA2_BYTES)?;
        }
        Ok(Some(path))
    }
    #[cfg(not(windows))]
    {
        Ok(find_on_path("aria2c"))
    }
}

#[cfg(not(windows))]
fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path) {
        for candidate in [name.to_string(), format!("{name}.exe")] {
            let path = directory.join(candidate);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

fn run_with_progress(
    mut command: Command,
    timeout: Duration,
    progress: &ProgressHandle,
    watch_path: &Path,
    name: &str,
    verb: &str,
) -> Result<std::process::ExitStatus> {
    let mut child = command
        .spawn()
        .with_context(|| format!("failed to start {verb}"))?;
    let started = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("failed to wait for {verb}"))?
        {
            return Ok(status);
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("{verb} timed out after {} seconds", timeout.as_secs());
        }
        if let Ok(metadata) = fs::metadata(watch_path)
            && metadata.is_file()
        {
            progress.set_message(&format!("{verb} {name} ({})", display_size(metadata.len())));
        } else {
            progress.set_message(&format!("{verb} {name}"));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

pub fn prepare_repository(
    spec: &str,
    destination: &Path,
    progress: &ProgressHandle,
    timeout: Duration,
) -> Result<PathBuf> {
    let local = Path::new(spec);
    if local.is_dir() {
        git_output(local, &["rev-parse", "--git-dir"])
            .with_context(|| format!("{} is not a Git repository", local.display()))?;
        return Ok(local.to_path_buf());
    }
    if local.exists() {
        anyhow::bail!(
            "--git-history requires Git repository directories or remote repository URLs; {} is a file",
            local.display()
        );
    }
    if !is_git_url(spec) && !spec.contains("://") {
        anyhow::bail!(
            "Git repository path does not exist: {spec}; use a directory or repository URL"
        );
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    progress.add_total(1);
    progress.set_message(&format!("Cloning {spec}"));
    let mut command = Command::new("git");
    command
        .args(["clone", "--mirror", "--filter=blob:none", "--quiet", "--"])
        .arg(spec)
        .arg(destination)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let status = run_with_progress(command, timeout, progress, destination, spec, "Cloning")?;
    if !status.success() {
        let _ = fs::remove_dir_all(destination);
        let mut fallback = Command::new("git");
        fallback
            .args(["clone", "--mirror", "--quiet", "--"])
            .arg(spec)
            .arg(destination)
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let status = run_with_progress(fallback, timeout, progress, destination, spec, "Cloning")?;
        if !status.success() {
            anyhow::bail!("git clone failed for {spec}");
        }
    }
    progress.inc(&format!("Cloned {spec}"));
    Ok(destination.to_path_buf())
}

pub fn history_value(repo: &Path) -> Result<Value> {
    let refs_text = git_output(repo, &["show-ref"]).unwrap_or_default();
    let mut refs = Map::new();
    for line in refs_text.lines() {
        let mut parts = line.split_whitespace();
        let Some(sha) = parts.next() else { continue };
        let Some(name) = parts.next() else { continue };
        refs.insert(name.to_string(), Value::String(sha.to_string()));
    }
    let head = git_output(repo, &["symbolic-ref", "--short", "HEAD"])
        .or_else(|_| git_output(repo, &["rev-parse", "HEAD"]))
        .unwrap_or_default()
        .trim()
        .to_string();
    let count = git_output(repo, &["rev-list", "--all", "--count"])
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(0);
    let log = git_output(
        repo,
        &[
            "log",
            "--all",
            &format!("--max-count={HISTORY_LIMIT}"),
            "--date=iso-strict",
            "--pretty=format:%H%x1f%an%x1f%ae%x1f%aI%x1f%s%x1e",
        ],
    )
    .unwrap_or_default();
    let mut commits = Map::new();
    for record in log.split('\u{1e}') {
        let record = record.trim_matches(['\r', '\n']);
        if record.is_empty() {
            continue;
        }
        let fields: Vec<&str> = record.splitn(5, '\u{1f}').collect();
        if fields.len() < 5 {
            continue;
        }
        commits.insert(
            fields[0].trim().to_string(),
            json!({
                "author": fields[1],
                "email": fields[2],
                "date": fields[3],
                "subject": fields[4],
            }),
        );
    }
    let shown = commits.len();
    Ok(json!({
        "format": "git-history",
        "head": head,
        "refs": refs,
        "commits": commits,
        "commit_count": count,
        "shown_commit_count": shown,
        "truncated": count > shown,
    }))
}

pub fn history_report(
    left_repo: &Path,
    right_repo: &Path,
    left_label: &str,
    right_label: &str,
    max_changes: usize,
    progress: &ProgressHandle,
) -> Result<DiffReport> {
    let started = Instant::now();
    progress.start("Comparing git histories");
    let left_value = history_value(left_repo)
        .with_context(|| format!("failed to read history from {}", left_repo.display()))?;
    let right_value = history_value(right_repo)
        .with_context(|| format!("failed to read history from {}", right_repo.display()))?;
    let diff = diff_core::diff::structured::compare_values(
        "git-history",
        &left_value,
        &right_value,
        max_changes,
    );
    let status = if diff.changes.is_empty() {
        DiffStatus::Equal
    } else {
        DiffStatus::Different
    };
    let equal = status == DiffStatus::Equal;
    let summary = Summary {
        files_compared: 1,
        files_equal: usize::from(equal),
        files_changed: usize::from(!equal),
        structured_changes: if equal { 0 } else { diff.changes.len() },
        ..Summary::default()
    };
    let left_count = left_value["commit_count"].as_u64().unwrap_or(0);
    let right_count = right_value["commit_count"].as_u64().unwrap_or(0);
    let mut warnings = Vec::new();
    if left_value["truncated"].as_bool().unwrap_or(false)
        || right_value["truncated"].as_bool().unwrap_or(false)
    {
        warnings.push(format!(
            "history detail is limited to the most recent {HISTORY_LIMIT} commits"
        ));
    }
    let root = DiffNode::new(
        format!("git history: {left_count} -> {right_count} commit(s)"),
        status,
        DiffBody::Structured(diff),
    );
    progress.finish();
    Ok(DiffReport {
        tool: TOOL_NAME,
        version: TOOL_VERSION,
        left: ArtifactInfo {
            path: left_label.to_string(),
            kind: FileKind::GitHistory,
            size: 0,
        },
        right: ArtifactInfo {
            path: right_label.to_string(),
            kind: FileKind::GitHistory,
            size: 0,
        },
        status,
        summary,
        root,
        warnings,
        elapsed_ms: started.elapsed().as_millis(),
        decompiled: None,
    })
}

fn git_output(repo: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if !output.status.success() {
        anyhow::bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_links_and_repository_urls() {
        assert!(is_http_link("https://example.com/file.zip"));
        assert!(is_http_link("http://example.com/file.zip?token=1"));
        assert!(!is_http_link("https://example.com/repo.git"));
        assert!(is_git_url("https://example.com/repo.git"));
        assert!(is_git_url("git@example.com:owner/repo.git"));
        assert!(is_git_url("ssh://git@example.com/repo.git"));
        assert!(is_git_url("file:///tmp/repo.git"));
        assert!(!is_git_url("https://example.com/repo"));
    }

    #[test]
    fn link_file_name_uses_the_last_path_segment() {
        assert_eq!(
            file_name("https://example.com/releases/tool.zip?download=1"),
            "tool.zip"
        );
        assert_eq!(file_name("https://example.com/"), "download.bin");
    }

    #[test]
    fn link_file_name_rejects_traversal_and_reserved_names() {
        assert_eq!(file_name("https://example.com/.."), "download.bin");
        assert_eq!(file_name("https://example.com/%2e%2e"), "download.bin");
        assert_eq!(file_name("https://example.com/NUL"), "download.bin");
        assert_eq!(file_name("https://example.com/com1.txt"), "download.bin");
        assert_eq!(file_name("https://example.com/trailing."), "download.bin");
    }

    #[test]
    fn parses_content_range_totals() {
        assert_eq!(parse_content_range("bytes 0-0/104857601"), Some(104857601));
        assert_eq!(parse_content_range("bytes 0-0/*"), None);
        assert_eq!(parse_content_range("invalid"), None);
    }

    #[test]
    fn yes_overrides_the_large_download_prompt() {
        assert!(check_size_limit(Some(LARGE_FILE_BYTES), true).is_ok());
        let error = check_size_limit(Some(LARGE_FILE_BYTES + 1), false).unwrap_err();
        assert!(error.to_string().contains("100 MiB"), "{error}");
        assert!(check_size_limit(None, false).is_ok());
    }
}
