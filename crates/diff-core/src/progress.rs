use std::fmt;
use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Thread-safe progress sink used by the engine and security scanner.
///
/// The handle is intentionally lightweight and writes only to stderr. When
/// disabled, every method is a no-op, so library callers pay no formatting
/// cost and reports are unaffected.
#[derive(Clone)]
pub struct ProgressHandle {
    inner: Arc<Inner>,
}

struct Inner {
    enabled: AtomicBool,
    label: Mutex<String>,
    total: AtomicUsize,
    current: AtomicUsize,
    file_mode: AtomicBool,
    file_total: AtomicU64,
    file_current: AtomicU64,
    last_render: Mutex<Instant>,
    spinner: AtomicUsize,
    ticker: Mutex<Option<Ticker>>,
}

struct Ticker {
    stop: Arc<AtomicBool>,
    join: JoinHandle<()>,
}

impl fmt::Debug for ProgressHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProgressHandle")
            .field("enabled", &self.enabled())
            .field("current", &self.current())
            .field("total", &self.total())
            .finish()
    }
}

impl ProgressHandle {
    pub fn new(enabled: bool) -> Self {
        Self {
            inner: Arc::new(Inner {
                enabled: AtomicBool::new(enabled),
                label: Mutex::new(String::new()),
                total: AtomicUsize::new(0),
                current: AtomicUsize::new(0),
                file_mode: AtomicBool::new(false),
                file_total: AtomicU64::new(0),
                file_current: AtomicU64::new(0),
                last_render: Mutex::new(Instant::now() - Duration::from_secs(1)),
                spinner: AtomicUsize::new(0),
                ticker: Mutex::new(None),
            }),
        }
    }

    pub fn auto() -> Self {
        Self::new(std::io::stderr().is_terminal())
    }

    pub fn disabled() -> Self {
        Self::new(false)
    }

    pub fn enabled(&self) -> bool {
        self.inner.enabled.load(Ordering::Relaxed)
    }

    pub fn current(&self) -> usize {
        self.inner.current.load(Ordering::Relaxed)
    }

    pub fn total(&self) -> usize {
        self.inner.total.load(Ordering::Relaxed)
    }

    pub fn start(&self, label: &str) {
        if !self.enabled() {
            return;
        }
        self.stop_ticker();
        self.inner.file_mode.store(false, Ordering::Relaxed);
        self.set_message(label);
        render(self, true);
    }

    /// Start a byte-oriented progress bar for one large file. A background
    /// ticker keeps the spinner moving while a parser is doing CPU work that
    /// does not report intermediate byte progress.
    pub fn start_file_analysis(&self, label: &str, total_bytes: u64) {
        if !self.enabled() {
            return;
        }
        self.stop_ticker();
        self.inner.total.store(0, Ordering::Relaxed);
        self.inner.current.store(0, Ordering::Relaxed);
        self.inner.file_total.store(total_bytes, Ordering::Relaxed);
        self.inner.file_current.store(0, Ordering::Relaxed);
        self.inner.file_mode.store(true, Ordering::Relaxed);
        if let Ok(mut current) = self.inner.label.lock() {
            *current = label.to_string();
        }
        render(self, true);
        self.start_ticker();
    }

    pub fn set_file_bytes(&self, current: u64) {
        if !self.enabled() {
            return;
        }
        self.inner.file_current.store(current, Ordering::Relaxed);
        render(self, false);
    }

    pub fn add_file_bytes(&self, delta: u64) {
        if !self.enabled() || delta == 0 {
            return;
        }
        self.inner.file_current.fetch_add(delta, Ordering::Relaxed);
        render(self, false);
    }

    fn start_ticker(&self) {
        let weak = Arc::downgrade(&self.inner);
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let join = thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(100));
                let Some(inner) = weak.upgrade() else {
                    break;
                };
                render_inner(&inner, false);
            }
        });
        if let Ok(mut ticker) = self.inner.ticker.lock() {
            *ticker = Some(Ticker { stop, join });
        }
    }

    fn stop_ticker(&self) {
        let ticker = self
            .inner
            .ticker
            .lock()
            .ok()
            .and_then(|mut slot| slot.take());
        if let Some(ticker) = ticker {
            ticker.stop.store(true, Ordering::Relaxed);
            let _ = ticker.join.join();
        }
    }

    pub fn set_message(&self, label: &str) {
        if !self.enabled() {
            return;
        }
        if let Ok(mut current) = self.inner.label.lock() {
            *current = label.to_string();
        }
        render(self, false);
    }

    pub fn add_total(&self, count: usize) {
        if self.enabled() && count > 0 {
            if self.inner.file_mode.swap(false, Ordering::Relaxed) {
                self.stop_ticker();
            }
            let previous = self.inner.total.fetch_add(count, Ordering::Relaxed);
            if previous == 0 {
                render(self, true);
            }
        }
    }

    pub fn inc(&self, label: &str) {
        if !self.enabled() {
            return;
        }
        self.inner.current.fetch_add(1, Ordering::Relaxed);
        self.set_message(label);
        render(self, false);
    }

    pub fn finish(&self) {
        if !self.enabled() {
            return;
        }
        self.stop_ticker();
        self.inner.file_mode.store(false, Ordering::Relaxed);
        self.inner.file_total.store(0, Ordering::Relaxed);
        self.inner.file_current.store(0, Ordering::Relaxed);
        let mut stderr = std::io::stderr().lock();
        let _ = write!(stderr, "\r{}\r", " ".repeat(100));
        let _ = stderr.flush();
    }

    /// Clear counters and the label before a new phase (for example downloads
    /// followed by comparison). The handle stays enabled.
    pub fn reset(&self) {
        if !self.enabled() {
            return;
        }
        self.stop_ticker();
        self.inner.file_mode.store(false, Ordering::Relaxed);
        self.inner.file_total.store(0, Ordering::Relaxed);
        self.inner.file_current.store(0, Ordering::Relaxed);
        self.inner.total.store(0, Ordering::Relaxed);
        self.inner.current.store(0, Ordering::Relaxed);
        if let Ok(mut label) = self.inner.label.lock() {
            label.clear();
        }
    }
}

fn render(handle: &ProgressHandle, force: bool) {
    render_inner(&handle.inner, force);
}

fn render_inner(inner: &Inner, force: bool) {
    if !inner.enabled.load(Ordering::Relaxed) {
        return;
    }
    let now = Instant::now();
    {
        let Ok(mut last) = inner.last_render.lock() else {
            return;
        };
        if !force && now.duration_since(*last) < Duration::from_millis(80) {
            return;
        }
        *last = now;
    }
    let label = inner
        .label
        .lock()
        .map(|label| label.clone())
        .unwrap_or_default();
    let spinner = ["|", "/", "-", "\\"][inner.spinner.fetch_add(1, Ordering::Relaxed) % 4];
    let label: String = label.chars().take(40).collect();
    let line = if inner.file_mode.load(Ordering::Relaxed) {
        let total = inner.file_total.load(Ordering::Relaxed);
        let current = inner.file_current.load(Ordering::Relaxed).min(total);
        if total > 0 {
            const BAR_WIDTH: usize = 20;
            let filled = ((current as u128 * BAR_WIDTH as u128) / total as u128) as usize;
            let bar = format!("[{}{}]", "#".repeat(filled), "-".repeat(BAR_WIDTH - filled));
            let percent = ((current as u128 * 100) / total as u128) as usize;
            format!(
                "{spinner} {label:<40}  {bar}  {:.1}/{:.1} MB  {percent:>3}%",
                current as f64 / (1024.0 * 1024.0),
                total as f64 / (1024.0 * 1024.0)
            )
        } else {
            format!("{spinner} {label}")
        }
    } else {
        let current = inner.current.load(Ordering::Relaxed);
        let total = inner.total.load(Ordering::Relaxed);
        if total > 0 {
            const BAR_WIDTH: usize = 20;
            let current = current.min(total);
            let filled = (current * BAR_WIDTH) / total;
            let bar = format!("[{}{}]", "#".repeat(filled), "-".repeat(BAR_WIDTH - filled));
            let percent = (current * 100) / total;
            format!("{spinner} {label:<40}  {bar}  {current}/{total}  {percent:>3}%")
        } else {
            format!("{spinner} {label}")
        }
    };
    let mut stderr = std::io::stderr().lock();
    let _ = write!(stderr, "\r{line:<100}");
    let _ = stderr.flush();
}
