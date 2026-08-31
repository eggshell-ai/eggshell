//! Central, in-memory application logger.
//!
//! The logger deliberately does not print to stdout. Consumers can read recent
//! entries or subscribe to entries created after the subscription.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use serde::Serialize;
use tokio::sync::broadcast;

const CHANNEL_CAPACITY: usize = 256;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, Serialize)]
pub struct LogEntry {
    pub level: LogLevel,
    pub message: String,
    pub sensitive: bool,
    /// Kept as a process-local value for now; consumers can format it later.
    #[serde(skip)]
    pub timestamp: SystemTime,
}

struct LoggerState {
    entries: VecDeque<LogEntry>,
    sender: broadcast::Sender<LogEntry>,
}

/// Thread-safe central logger shared by application services.
#[derive(Clone)]
pub struct Logger {
    state: Arc<Mutex<LoggerState>>,
}

impl Default for Logger {
    fn default() -> Self {
        Self::new()
    }
}

impl Logger {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self {
            state: Arc::new(Mutex::new(LoggerState {
                entries: VecDeque::new(),
                sender,
            })),
        }
    }

    /// The process-wide logger, so code without a [`Logger`] handed to it —
    /// dependency checks, shells, dev-only mocks — still accumulates its output
    /// in the same central place instead of the terminal.
    pub fn global() -> Logger {
        static GLOBAL: std::sync::OnceLock<Logger> = std::sync::OnceLock::new();
        GLOBAL.get_or_init(Logger::new).clone()
    }

    pub fn info(&self, message: impl Into<String>, sensitive: bool) {
        self.push(LogLevel::Info, message.into(), sensitive);
    }

    pub fn warning(&self, message: impl Into<String>, sensitive: bool) {
        self.push(LogLevel::Warning, message.into(), sensitive);
    }

    pub fn error(&self, message: impl Into<String>, sensitive: bool) {
        self.push(LogLevel::Error, message.into(), sensitive);
    }

    /// Returns at most the last `n` entries. Sensitive entries are excluded by default.
    pub fn read(&self, n: usize, include_sensitive: bool) -> Vec<LogEntry> {
        let state = self.state.lock().expect("logger mutex poisoned");
        state
            .entries
            .iter()
            .filter(|entry| include_sensitive || !entry.sensitive)
            .rev()
            .take(n)
            .cloned()
            .collect()
    }

    /// Subscribes to new entries only. The returned stream filters sensitive entries.
    pub fn stream(&self, include_sensitive: bool) -> LogStream {
        let receiver = self
            .state
            .lock()
            .expect("logger mutex poisoned")
            .sender
            .subscribe();
        LogStream {
            receiver,
            include_sensitive,
        }
    }

    /// Pushes a log entry. `sensitive` entries are hidden from non-privileged streams.
    pub fn push(&self, level: LogLevel, message: String, sensitive: bool) {
        let entry = LogEntry {
            level,
            message,
            sensitive,
            timestamp: SystemTime::now(),
        };
        let mut state = self.state.lock().expect("logger mutex poisoned");
        state.entries.push_back(entry.clone());
        let _ = state.sender.send(entry);
    }
}

pub struct LogStream {
    receiver: broadcast::Receiver<LogEntry>,
    include_sensitive: bool,
}

impl LogStream {
    /// Waits for the next permitted log entry. Lagged entries are skipped.
    pub async fn next(&mut self) -> Option<LogEntry> {
        loop {
            match self.receiver.recv().await {
                Ok(entry) if self.include_sensitive || !entry.sensitive => return Some(entry),
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }
}

// Examples:
// let logger = Logger::new();
// logger.info("server started", false);
// logger.warning("request contains a token", true);
// logger.error("database unavailable", false);
// let recent = logger.read(10, false);
// let mut live = logger.stream(false);
// let next_entry = live.next().await;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reads_and_streams_with_sensitive_filtering() {
        let logger = Logger::new();
        let mut public_stream = logger.stream(false);
        let mut private_stream = logger.stream(true);

        logger.info("public", false);
        logger.error("private", true);

        assert_eq!(logger.read(10, false).len(), 1);
        assert_eq!(logger.read(10, true).len(), 2);
        assert_eq!(public_stream.next().await.unwrap().message, "public");
        assert_eq!(private_stream.next().await.unwrap().message, "public");
        assert_eq!(private_stream.next().await.unwrap().message, "private");
    }
}
