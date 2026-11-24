//! Logging setup: console + log file + in-memory ring buffer for the in-game debug console.
//!
//! The `log` facade is process-global by design, so the ring buffer is shared through an
//! `Arc<Mutex<..>>` handle returned from [`init`] rather than through a mutable global.

use std::collections::VecDeque;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Recent log lines, newest last.
#[derive(Clone, Default)]
pub struct LogBuffer {
    inner: Arc<Mutex<VecDeque<LogLine>>>,
}

#[derive(Clone, Debug)]
pub struct LogLine {
    pub level: log::Level,
    pub text: String,
}

const LOG_BUFFER_LINES: usize = 500;

impl LogBuffer {
    fn push(&self, level: log::Level, text: String) {
        if let Ok(mut lines) = self.inner.lock() {
            if lines.len() >= LOG_BUFFER_LINES {
                lines.pop_front();
            }
            lines.push_back(LogLine { level, text });
        }
    }

    /// Copies the most recent `max` lines.
    pub fn recent(&self, max: usize) -> Vec<LogLine> {
        match self.inner.lock() {
            Ok(lines) => lines.iter().rev().take(max).rev().cloned().collect(),
            Err(_) => Vec::new(),
        }
    }
}

struct TeeWriter {
    file: Option<File>,
}

impl Write for TeeWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stderr().write_all(buf);
        if let Some(file) = self.file.as_mut() {
            let _ = file.write_all(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let _ = std::io::stderr().flush();
        if let Some(file) = self.file.as_mut() {
            file.flush()?;
        }
        Ok(())
    }
}

/// Default filter: our crates at `info`, noisy GPU internals at `warn`. Override with `RUST_LOG`.
pub const DEFAULT_FILTER: &str = "info,wgpu_core=warn,wgpu_hal=warn,naga=warn,calloop=warn";

/// Initialises logging. `log_file` is created/truncated; failures to open it are reported but
/// not fatal. Safe to call more than once (later calls only return a new, unused buffer).
pub fn init(log_file: Option<&Path>) -> LogBuffer {
    let buffer = LogBuffer::default();
    let file = log_file.and_then(|path| {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match File::create(path) {
            Ok(f) => Some(f),
            Err(err) => {
                eprintln!("warning: could not create log file {}: {err}", path.display());
                None
            }
        }
    });

    let ring = buffer.clone();
    let mut builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(DEFAULT_FILTER));
    builder
        .target(env_logger::Target::Pipe(Box::new(TeeWriter { file })))
        .format(move |f, record| {
            let line = format!("[{:<5} {}] {}", record.level(), record.target(), record.args());
            ring.push(record.level(), line.clone());
            writeln!(f, "{line}")
        });
    if builder.try_init().is_err() {
        log::debug!("logger already initialised");
    }
    install_panic_hook();
    buffer
}

fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown location".to_owned());
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "non-string panic payload".to_owned());
        log::error!("PANIC at {location}: {payload}");
        log::logger().flush();
        default_hook(info);
    }));
}
