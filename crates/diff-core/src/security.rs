use crate::progress::ProgressHandle;
use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
#[cfg(windows)]
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct KasperskyInstallation {
    pub executable: PathBuf,
    pub service_running: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanOutcome {
    Clean,
    Threats(Vec<String>),
    Unavailable(String),
    Failed(String),
}

pub fn detect_kaspersky(explicit: Option<&Path>) -> Option<KasperskyInstallation> {
    if let Some(path) = explicit {
        return path.is_file().then(|| KasperskyInstallation {
            executable: path.to_path_buf(),
            service_running: None,
        });
    }

    #[cfg(windows)]
    {
        let mut roots = Vec::new();
        for variable in ["ProgramFiles(x86)", "ProgramFiles", "ProgramW6432"] {
            if let Some(root) = std::env::var_os(variable) {
                roots.push(PathBuf::from(root).join("Kaspersky Lab"));
            }
        }
        let mut candidates = Vec::new();
        for root in roots {
            if !root.is_dir() {
                continue;
            }
            for entry in WalkDir::new(&root).max_depth(3).into_iter().flatten() {
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if name == "avp.com" || name == "avp.exe" {
                    candidates.push(entry.path().to_path_buf());
                }
            }
        }
        candidates.sort_by(|left, right| {
            let left_com = left.extension().is_some_and(|ext| ext == "com");
            let right_com = right.extension().is_some_and(|ext| ext == "com");
            right_com.cmp(&left_com)
        });
        let executable = candidates.into_iter().next()?;
        Some(KasperskyInstallation {
            executable,
            service_running: kaspersky_service_running(),
        })
    }

    #[cfg(not(windows))]
    {
        let _ = explicit;
        None
    }
}

#[cfg(windows)]
fn kaspersky_service_running() -> Option<bool> {
    let output = Command::new("sc.exe")
        .args(["query", "type=", "service", "state=", "all"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut in_kaspersky = false;
    for line in text.lines() {
        if line.contains("SERVICE_NAME:") {
            in_kaspersky = line.to_ascii_uppercase().contains("AVP");
            continue;
        }
        if in_kaspersky && line.contains("STATE") {
            let state = line
                .split_once(':')
                .and_then(|(_, value)| value.split_whitespace().next())
                .and_then(|value| value.parse::<u32>().ok());
            return state.map(|value| value == 4);
        }
    }
    None
}

/// Scan local paths with the installed Kaspersky command-line API.
///
/// Kaspersky is only used when it is installed and detected. If an explicit
/// executable is supplied, the service-state check is skipped (useful for
/// managed installations and tests).
pub fn scan_paths(
    explicit: Option<&Path>,
    paths: &[&Path],
    timeout: Duration,
    progress: Option<&ProgressHandle>,
) -> ScanOutcome {
    let Some(installation) = detect_kaspersky(explicit) else {
        return ScanOutcome::Unavailable(
            "Kaspersky installation was not detected; security scan skipped".to_string(),
        );
    };
    if explicit.is_none() && installation.service_running == Some(false) {
        return ScanOutcome::Unavailable(
            "Kaspersky is installed but its service is not running; start Kaspersky or pass --kaspersky <avp.com>"
                .to_string(),
        );
    }
    if let Some(progress) = progress {
        progress.add_total(1);
        progress.set_message(&format!("Kaspersky scanning {} object(s)", paths.len()));
    }
    let result = scan_many(&installation.executable, paths, timeout);
    if let Some(progress) = progress {
        progress.inc("Kaspersky scan complete");
    }
    match result {
        Ok(report) if report.threats.is_empty() => ScanOutcome::Clean,
        Ok(report) => ScanOutcome::Threats(report.threats),
        Err(error) => ScanOutcome::Failed(error),
    }
}

#[derive(Debug, Default)]
struct Report {
    threats: Vec<String>,
}

fn scan_many(executable: &Path, paths: &[&Path], timeout: Duration) -> Result<Report, String> {
    let directory = tempfile::tempdir().map_err(|error| format!("Kaspersky temp dir: {error}"))?;
    let report_path = directory.path().join("kaspersky-report.txt");
    let mut command = kaspersky_command(executable);
    command
        .arg("SCAN")
        .arg(format!("/R:{}", report_path.display()))
        .arg("/i0");
    for path in paths {
        command.arg(path);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let status = run_with_timeout(&mut command, timeout)?;
    let data = std::fs::File::open(&report_path)
        .ok()
        .and_then(|file| {
            let mut data = Vec::new();
            file.take(4 * 1024 * 1024)
                .read_to_end(&mut data)
                .ok()
                .map(|_| data)
        })
        .unwrap_or_default();
    let text = decode_report(&data);
    if text.trim().is_empty() {
        return Err("Kaspersky returned no report".to_string());
    }
    let report = parse_report(&text);
    if !status.success() && report.threats.is_empty() {
        return Err(format!(
            "Kaspersky scan exited with {}",
            status.code().unwrap_or(-1)
        ));
    }
    Ok(report)
}

fn kaspersky_command(executable: &Path) -> Command {
    let extension = executable
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    #[cfg(windows)]
    if extension == "cmd" || extension == "bat" {
        let mut command = Command::new("cmd.exe");
        command.arg("/C").arg(executable);
        return command;
    }
    #[cfg(not(windows))]
    if extension == "sh" {
        let mut command = Command::new("sh");
        command.arg(executable);
        return command;
    }
    Command::new(executable)
}

fn run_with_timeout(
    command: &mut Command,
    timeout: Duration,
) -> Result<std::process::ExitStatus, String> {
    let mut child = command
        .spawn()
        .map_err(|error| format!("Kaspersky start: {error}"))?;
    let started = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("Kaspersky wait: {error}"))?
        {
            return Ok(status);
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "Kaspersky scan timed out after {} seconds",
                timeout.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn decode_report(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter_map(|pair| match pair {
                [first, second] => Some(u16::from_le_bytes([*first, *second])),
                _ => None,
            })
            .collect();
        return String::from_utf16_lossy(&units);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter_map(|pair| match pair {
                [first, second] => Some(u16::from_be_bytes([*first, *second])),
                _ => None,
            })
            .collect();
        return String::from_utf16_lossy(&units);
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => String::from_utf8_lossy(bytes).into_owned(),
    }
}

fn parse_report(text: &str) -> Report {
    let lower = text.to_ascii_lowercase();
    let clean = [
        "no threats",
        "threats detected: 0",
        "detected: 0",
        "0 threats",
        "no malware",
        "未检测到",
        "没有检测到",
        "无威胁",
    ]
    .iter()
    .any(|marker| lower.contains(marker));
    if clean {
        return Report::default();
    }
    let markers = [
        "detected",
        "threat",
        "infected",
        "malware",
        "virus",
        "trojan",
        "heur:",
        "pdm:",
        "uds:",
        "exploit",
        "检测到",
        "威胁",
        "病毒",
        "恶意",
    ];
    let mut threats = BTreeSet::new();
    for line in text.lines() {
        let line_lower = line.to_ascii_lowercase();
        if line_lower.contains("0 detected") || line_lower.contains("detected: 0") {
            continue;
        }
        let has_marker = markers.iter().any(|marker| line_lower.contains(marker));
        let looks_like_path = line.contains(":\\") || line.contains("\\\\") || line.contains('/');
        if has_marker && looks_like_path {
            let line = line.trim();
            if !line.is_empty() {
                threats.insert(line.to_string());
            }
        }
        if threats.len() >= 20 {
            break;
        }
    }
    Report {
        threats: threats.into_iter().collect(),
    }
}

#[cfg(test)]
#[path = "../tests/unit/security.rs"]
mod tests;
