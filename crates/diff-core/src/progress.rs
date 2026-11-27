use std::fmt;
use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
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
    last_render: Mutex<Instant>,
    spinner: AtomicUsize,
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
                last_render: Mutex::new(Instant::now() - Duration::from_secs(1)),
                spinner: AtomicUsize::new(0),
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
        self.set_message(label);
        render(self, true);
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
        self.inner.total.store(0, Ordering::Relaxed);
        self.inner.current.store(0, Ordering::Relaxed);
        if let Ok(mut label) = self.inner.label.lock() {
            label.clear();
        }
    }
}

fn render(handle: &ProgressHandle, force: bool) {
    if !handle.enabled() {
        return;
    }
    let now = Instant::now();
    {
        let Ok(mut last) = handle.inner.last_render.lock() else {
            return;
        };
        if !force && now.duration_since(*last) < Duration::from_millis(80) {
            return;
        }
        *last = now;
    }
    let label = handle
        .inner
        .label
        .lock()
        .map(|label| label.clone())
        .unwrap_or_default();
    let current = handle.inner.current.load(Ordering::Relaxed);
    let total = handle.inner.total.load(Ordering::Relaxed);
    let spinner = ["|", "/", "-", "\\"][handle.inner.spinner.fetch_add(1, Ordering::Relaxed) % 4];
    let mut stderr = std::io::stderr().lock();
    let _ = if total > 0 {
        write!(stderr, "\r{spinner} {current}/{total} {label:.<80}")
    } else {
        write!(stderr, "\r{spinner} {label:.<90}")
    };
    let _ = stderr.flush();
}
