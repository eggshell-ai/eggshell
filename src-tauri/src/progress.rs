//! The log Eggshell streams to the window while it works: dependency installs on
//! the setup screen, and the shells a new project is built from.
//!
//! One [`ProgressLog`] carries several *channels* over a shared sequence counter,
//! so a window showing more than one stream — the backend and frontend tabs during
//! project creation — can order and de-duplicate every line it receives.

use serde::Serialize;
use std::collections::VecDeque;
use std::io::{BufReader, Read};
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::Emitter;

/// How much of the past the log keeps for a window that has not been opened yet.
/// Installers are chatty, and only the recent past explains what they did.
const LOG_CAPACITY: usize = 800;

/// Beyond this a line is a wall of text rather than information — winget prints
/// download URLs and PowerShell prints whole scripts back.
const LINE_LIMIT: usize = 400;

/// One line of output. `channel` says which stream it belongs to — `setup`, or
/// `symfony`/`react` while a project is being created — and `stream` says what
/// produced it: `command` for a command about to run, `stdout`/`stderr` for its
/// output, `info`/`error` for Eggshell's own reporting, and `done` for a channel
/// that has finished its work. The window tabs by the first and colours by the
/// second.
#[derive(Clone, Debug, Serialize)]
pub struct LogLine {
    pub seq: u64,
    pub channel: &'static str,
    pub stream: &'static str,
    pub text: String,
}

#[derive(Default)]
struct LogState {
    next_seq: u64,
    lines: VecDeque<LogLine>,
}

/// A running commentary: every line is printed for the terminal *and* emitted to
/// the window under `event`, and the last [`LOG_CAPACITY`] lines are retained so a
/// log opened halfway through — or after the launch-time MySQL start-up nobody was
/// listening for — still shows what it missed.
///
/// Cloning is cheap, which is what lets a command running on a blocking task own a
/// copy.
#[derive(Clone)]
pub struct ProgressLog {
    app: tauri::AppHandle,
    event: &'static str,
    channel: &'static str,
    state: Arc<Mutex<LogState>>,
}

impl ProgressLog {
    pub fn new(app: tauri::AppHandle, event: &'static str, channel: &'static str) -> Self {
        Self { app, event, channel, state: Arc::new(Mutex::new(LogState::default())) }
    }

    /// The same log under a different channel. The sequence counter is shared, so
    /// lines from every channel stay in one order however many of them there are.
    pub fn for_channel(&self, channel: &'static str) -> Self {
        Self { app: self.app.clone(), event: self.event, channel, state: Arc::clone(&self.state) }
    }

    pub fn line(&self, stream: &'static str, text: impl Into<String>) {
        let text = text.into();
        println!("{}[{stream}]: {text}", self.channel);

        let line = {
            let mut state = self.lock();
            state.next_seq += 1;
            let line = LogLine { seq: state.next_seq, channel: self.channel, stream, text };
            if state.lines.len() == LOG_CAPACITY {
                state.lines.pop_front();
            }
            state.lines.push_back(line.clone());
            line
        };
        // No window to emit to is not a reason to stop working.
        let _ = self.app.emit(self.event, line);
    }

    pub fn history(&self) -> Vec<LogLine> {
        self.lock().lines.iter().cloned().collect()
    }

    /// A panicking logger would take the work it is reporting on down with it, and a
    /// poisoned lock costs at most one garbled line, so the guard is recovered rather
    /// than unwrapped.
    fn lock(&self) -> MutexGuard<'_, LogState> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Reads one of a child process's pipes to the end, logging each line as it
/// arrives rather than waiting for the command to exit, and returns the lines it
/// kept so a failure can quote them.
///
/// Lines are split on `\r` as well as `\n`: winget, npm and PowerShell redraw
/// progress with carriage returns, and a single endless line would tell nobody
/// anything.
pub fn pump_output(source: impl Read, stream: &'static str, log: &ProgressLog) -> Vec<String> {
    let mut reader = BufReader::new(source);
    let mut pending: Vec<u8> = Vec::new();
    let mut kept: Vec<String> = Vec::new();
    let mut chunk = [0u8; 4096];

    loop {
        let read = match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) => {
                log.line("error", format!("could not read {stream}: {error}"));
                break;
            }
        };
        pending.extend_from_slice(&chunk[..read]);
        while let Some(position) = pending.iter().position(|byte| matches!(byte, b'\n' | b'\r')) {
            let line: Vec<u8> = pending.drain(..=position).collect();
            if let Some(text) = presentable(&line) {
                log.line(stream, text.clone());
                kept.push(text);
            }
        }
    }

    // Whatever the command left without a closing newline.
    if let Some(text) = presentable(&pending) {
        log.line(stream, text.clone());
        kept.push(text);
    }
    kept
}

/// Turns one raw chunk of output into something worth showing, or `None` when
/// nothing is left: installers draw progress with escape sequences and block
/// characters, and a log full of `████ 71%` is worse than no log. Output is also
/// not guaranteed to be UTF-8, hence the lossy conversion.
fn presentable(bytes: &[u8]) -> Option<String> {
    let text = strip_ansi(&String::from_utf8_lossy(bytes));
    let trimmed = text.trim();
    if !trimmed.chars().any(char::is_alphanumeric) {
        return None;
    }

    let mut line: String = trimmed.chars().take(LINE_LIMIT).collect();
    if trimmed.chars().count() > LINE_LIMIT {
        line.push('…');
    }
    Some(line)
}

/// Drops ANSI escape sequences and stray control characters, so the log shows
/// text instead of the colours and cursor moves a command meant for a console.
fn strip_ansi(text: &str) -> String {
    let mut kept = String::with_capacity(text.len());
    let mut characters = text.chars();

    while let Some(character) = characters.next() {
        if character != '\u{1b}' {
            if !character.is_control() || character == '\t' {
                kept.push(character);
            }
            continue;
        }
        match characters.next() {
            // A CSI sequence runs until its final letter: `\u{1b}[32m`, `\u{1b}[2K`.
            Some('[') => {
                for next in characters.by_ref() {
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            // An OSC sequence carries a string terminated by BEL or ESC — winget
            // sets the console title that way.
            Some(']') => {
                for next in characters.by_ref() {
                    if next == '\u{7}' || next == '\u{1b}' {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    kept
}
