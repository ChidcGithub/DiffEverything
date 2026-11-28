use anyhow::Error;
use std::fmt;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

static NO_COLOR: AtomicBool = AtomicBool::new(false);

pub fn set_no_color(value: bool) {
    NO_COLOR.store(value, Ordering::Relaxed);
}

pub fn print(error: &Error) {
    let color = !NO_COLOR.load(Ordering::Relaxed)
        && std::env::var_os("NO_COLOR").is_none()
        && io::stderr().is_terminal();
    if let Some(input) = error.downcast_ref::<InputError>() {
        print_input(input, color);
    } else {
        print_generic(error, color);
    }
}

#[derive(Debug)]
pub struct InputError {
    label: &'static str,
    path: PathBuf,
    kind: io::ErrorKind,
}

impl fmt::Display for InputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.title(), self.path.display())
    }
}

impl std::error::Error for InputError {}

impl InputError {
    fn title(&self) -> &'static str {
        match self.kind {
            io::ErrorKind::NotFound => "input not found",
            io::ErrorKind::PermissionDenied => "input is not accessible",
            _ => "input cannot be opened",
        }
    }
}

pub fn ensure_exists(label: &'static str, path: &Path) -> anyhow::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(()),
        Err(error) => Err(InputError {
            label,
            path: path.to_path_buf(),
            kind: error.kind(),
        }
        .into()),
    }
}

fn print_input(input: &InputError, color: bool) {
    eprintln!("{} {}", paint("error:", ERROR, color), input.title());
    eprintln!(
        "  {} {}",
        paint(&format!("{}:", input.label), LABEL, color),
        input.path.display()
    );
    if input.kind == io::ErrorKind::NotFound {
        eprintln!(
            "  {} check that the path exists and is spelled correctly",
            paint("hint:", HINT, color)
        );
    } else if input.kind == io::ErrorKind::PermissionDenied {
        eprintln!(
            "  {} check file permissions or run with the required access",
            paint("hint:", HINT, color)
        );
    }
}

fn print_generic(error: &Error, color: bool) {
    let chain: Vec<&(dyn std::error::Error + 'static)> = error.chain().collect();
    let title = chain
        .first()
        .map(|cause| cause.to_string())
        .unwrap_or_else(|| "unexpected error".to_string());
    let io_error = chain
        .iter()
        .find_map(|cause| cause.downcast_ref::<io::Error>());

    eprintln!("{} {}", paint("error:", ERROR, color), title);

    let mut printed = Vec::new();
    for cause in chain.iter().skip(1) {
        let text = if let Some(io) = cause.downcast_ref::<io::Error>() {
            describe_io(io)
        } else {
            let text = cause.to_string();
            if io_error.is_some() && text == "I/O error" {
                continue;
            }
            text
        };
        if text.is_empty() || text == title || printed.contains(&text) {
            continue;
        }
        printed.push(text.clone());
        eprintln!("  {} {}", paint("cause:", LABEL, color), text);
    }

    if let Some(io) = io_error
        && let Some(hint) = io_hint(io.kind())
    {
        eprintln!("  {} {hint}", paint("hint:", HINT, color));
    }
}

fn describe_io(error: &io::Error) -> String {
    let description = match error.kind() {
        io::ErrorKind::NotFound => "file or directory not found",
        io::ErrorKind::PermissionDenied => "permission denied",
        io::ErrorKind::AlreadyExists => "path already exists",
        io::ErrorKind::InvalidInput => "invalid input",
        io::ErrorKind::InvalidData => "invalid or malformed data",
        io::ErrorKind::UnexpectedEof => "unexpected end of file",
        io::ErrorKind::TimedOut => "operation timed out",
        io::ErrorKind::WriteZero => "write failed",
        io::ErrorKind::Interrupted => "operation interrupted",
        io::ErrorKind::WouldBlock => "operation would block",
        _ => return error.to_string(),
    };
    match error.raw_os_error() {
        Some(code) => format!("{description} (os error {code})"),
        None => description.to_string(),
    }
}

fn io_hint(kind: io::ErrorKind) -> Option<&'static str> {
    match kind {
        io::ErrorKind::NotFound => Some("check that the path exists and is spelled correctly"),
        io::ErrorKind::PermissionDenied => {
            Some("check file permissions or run with the required access")
        }
        io::ErrorKind::InvalidData | io::ErrorKind::UnexpectedEof => {
            Some("the input may be truncated or malformed; try --detail concise")
        }
        io::ErrorKind::TimedOut => {
            Some("increase --download-timeout or --scan-timeout for slow operations")
        }
        _ => None,
    }
}

const ERROR: &str = "\x1b[1;31m";
const LABEL: &str = "\x1b[2m";
const HINT: &str = "\x1b[1;36m";
const RESET: &str = "\x1b[0m";

fn paint(text: &str, code: &str, color: bool) -> String {
    if color {
        format!("{code}{text}{RESET}")
    } else {
        text.to_string()
    }
}
