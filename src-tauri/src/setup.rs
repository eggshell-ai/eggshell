//! Dependency setup helpers, and the log the setup screen streams through.

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

/// One line of setup output. `stream` says what produced it — `command` for a
/// command about to run, `stdout`/`stderr` for its output, `info`/`error` for
/// Eggshell's own reporting — and the setup screen colours by it.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct LogLine {
    pub seq: u64,
    pub stream: &'static str,
    pub text: String,
}

#[derive(Default)]
struct LogState {
    next_seq: u64,
    lines: VecDeque<LogLine>,
}

/// Setup's running commentary: every line is printed for the terminal *and*
/// emitted to the window as `setup-log`, and the last [`LOG_CAPACITY`] lines are
/// retained so a log window opened halfway through an install — or after the
/// launch-time MySQL start-up nobody was listening for — still shows what it
/// missed.
///
/// Cloning is cheap, which is what lets an installer running on a blocking task
/// own a copy.
#[derive(Clone)]
pub(crate) struct SetupLog {
    app: tauri::AppHandle,
    state: Arc<Mutex<LogState>>,
}

impl SetupLog {
    pub(crate) fn new(app: tauri::AppHandle) -> Self {
        Self { app, state: Arc::new(Mutex::new(LogState::default())) }
    }

    pub(crate) fn line(&self, stream: &'static str, text: impl Into<String>) {
        let text = text.into();
        println!("setup[{stream}]: {text}");

        let line = {
            let mut state = self.lock();
            state.next_seq += 1;
            let line = LogLine { seq: state.next_seq, stream, text };
            if state.lines.len() == LOG_CAPACITY {
                state.lines.pop_front();
            }
            state.lines.push_back(line.clone());
            line
        };
        // No window to emit to is not a reason to stop installing.
        let _ = self.app.emit("setup-log", line);
    }

    pub(crate) fn history(&self) -> Vec<LogLine> {
        self.lock().lines.iter().cloned().collect()
    }

    /// A panicking logger would take the install down with it, and a poisoned lock
    /// costs at most one garbled line, so the guard is recovered rather than
    /// unwrapped.
    fn lock(&self) -> MutexGuard<'_, LogState> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Reads one of a child process's pipes to the end, logging each line as it
/// arrives rather than waiting for the command to exit, and returns the lines it
/// kept so a failure can quote them.
///
/// Lines are split on `\r` as well as `\n`: winget and PowerShell redraw progress
/// with carriage returns, and a single endless line would tell nobody anything.
pub(crate) fn pump_output(source: impl Read, stream: &'static str, log: &SetupLog) -> Vec<String> {
    let mut reader = BufReader::new(source);
    let mut pending: Vec<u8> = Vec::new();
    let mut kept: Vec<String> = Vec::new();
    let mut chunk = [0u8; 4096];

    loop {
        let read = match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) => {
                log.line("error", format!("could not read installer {stream}: {error}"));
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
/// text instead of the colours and cursor moves an installer meant for a console.
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

/// The portable build setup falls back to, named once: the archive unpacks into a
/// directory of the same name, and `lib.rs` starts the server from inside it.
#[cfg(windows)]
pub(crate) const MYSQL_DIRECTORY: &str = "mysql-8.0.46-winx64";

pub(crate) const PHP_DIRECTORY: &str = "php";

/// The Node archive contains this one top-level directory. Keeping that name
/// means the fallback can extract directly into Eggshell's app-data directory.
#[cfg(windows)]
pub(crate) const NODE_DIRECTORY: &str = "node-v24.19.0-win-x64";

#[cfg(windows)]
pub(crate) fn managed_node_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("APPDATA")
        .map(|root| std::path::PathBuf::from(root).join("eggshell").join(NODE_DIRECTORY))
}

#[cfg(windows)]
pub(crate) fn managed_node_present() -> bool {
    managed_node_dir().is_some_and(|directory| directory.join("node.exe").is_file())
}

pub(crate) fn managed_php_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("APPDATA")
        .map(|root| std::path::PathBuf::from(root).join("eggshell").join(PHP_DIRECTORY))
}

pub(crate) fn managed_php_present() -> bool {
    managed_php_dir().is_some_and(|directory| directory.join("php.exe").is_file())
}

/// Where the fallback route leaves the server: the base directory holding
/// `bin/mysqld.exe`, `my.ini` and `data`. `None` on the platforms where setup
/// installs MySQL through a package manager instead, since those register a
/// service that owns the daemon.
#[cfg(windows)]
pub(crate) fn managed_mysql_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(|root| std::path::PathBuf::from(root).join("MySQL").join(MYSQL_DIRECTORY))
}

#[cfg(not(windows))]
pub(crate) fn managed_mysql_dir() -> Option<std::path::PathBuf> { None }

#[cfg(windows)]
pub(crate) fn mysql_fallback_command() -> Vec<String> {
    let url = format!("https://cdn.mysql.com/Downloads/MySQL-8.0/{MYSQL_DIRECTORY}.zip");
    let script = format!(r#"$ErrorActionPreference = 'Stop'; $root = Join-Path $env:LOCALAPPDATA 'MySQL'; $base = Join-Path $root '{MYSQL_DIRECTORY}'; $archive = Join-Path $root '{MYSQL_DIRECTORY}.zip'; $data = Join-Path $base 'data'; New-Item -ItemType Directory -Force -Path $root | Out-Null; if (-not (Test-Path (Join-Path $base 'bin\mysqld.exe'))) {{ [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; Invoke-WebRequest -Uri '{url}' -OutFile $archive -UseBasicParsing; Expand-Archive -LiteralPath $archive -DestinationPath $root -Force; Remove-Item -LiteralPath $archive -Force }}; New-Item -ItemType Directory -Force -Path $data | Out-Null; $baseForIni = $base.Replace('\', '/'); $dataForIni = $data.Replace('\', '/'); @"
[mysqld]
basedir     = $baseForIni
datadir     = $dataForIni
port        = 3306

[client]
port        = 3306
"@ | Set-Content -LiteralPath (Join-Path $base 'my.ini') -Encoding ascii; if (-not (Test-Path (Join-Path $data 'mysql'))) {{ & (Join-Path $base 'bin\mysqld.exe') "--defaults-file=$(Join-Path $base 'my.ini')" '--initialize-insecure'; if ($LASTEXITCODE -ne 0) {{ throw "mysqld initialization failed with exit code $LASTEXITCODE" }} }}"#);
    vec!["powershell".into(), "-NoProfile".into(), "-NonInteractive".into(), "-ExecutionPolicy".into(), "Bypass".into(), "-Command".into(), script]
}

#[cfg(windows)]
pub(crate) fn php_fallback_command() -> Vec<String> {
    let url = "https://downloads.php.net/~windows/releases/php-8.5.9-nts-Win32-vs17-x64.zip";
    let settings = "extension_dir = \"ext\"`r`nextension=curl`r`nextension=fileinfo`r`nextension=gd`r`nextension=intl`r`nextension=mbstring`r`nextension=openssl`r`nextension=pdo_mysql`r`nextension=pdo_pgsql`r`nextension=pdo_sqlite`r`nextension=sodium`r`nextension=sqlite3`r`nextension=xsl`r`nextension=zip";
    let script = format!(r#"$ErrorActionPreference = 'Stop'; $root = Join-Path $env:APPDATA 'eggshell'; $base = Join-Path $root '{PHP_DIRECTORY}'; $archive = Join-Path $root 'php-8.5.9.zip'; New-Item -ItemType Directory -Force -Path $root | Out-Null; if (-not (Test-Path (Join-Path $base 'php.exe'))) {{ Invoke-WebRequest -Uri '{url}' -OutFile $archive -UseBasicParsing; New-Item -ItemType Directory -Force -Path $base | Out-Null; Expand-Archive -LiteralPath $archive -DestinationPath $base -Force; Remove-Item -LiteralPath $archive -Force }}; $ini = Join-Path $base 'php.ini'; Move-Item -LiteralPath (Join-Path $base 'php.ini-development') -Destination $ini -Force; Add-Content -LiteralPath $ini -Value "`r`n{settings}`r`n" -Encoding ascii"#);
    vec!["powershell".into(), "-NoProfile".into(), "-NonInteractive".into(), "-ExecutionPolicy".into(), "Bypass".into(), "-Command".into(), script]
}

/// Downloads the pinned portable Node build when winget is unavailable (or its
/// installer fails). The archive's sole directory is extracted beneath AppData,
/// leaving `node.exe` and `npm.cmd` together in Eggshell's managed location.
#[cfg(windows)]
pub(crate) fn node_fallback_command() -> Vec<String> {
    let url = "https://nodejs.org/dist/v24.19.0/node-v24.19.0-win-x64.zip";
    let script = format!(r#"$ErrorActionPreference = 'Stop'; $root = Join-Path $env:APPDATA 'eggshell'; $base = Join-Path $root '{NODE_DIRECTORY}'; $archive = Join-Path $root 'node-v24.19.0-win-x64.zip'; New-Item -ItemType Directory -Force -Path $root | Out-Null; if (-not (Test-Path (Join-Path $base 'node.exe'))) {{ [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; Invoke-WebRequest -Uri '{url}' -OutFile $archive -UseBasicParsing; Expand-Archive -LiteralPath $archive -DestinationPath $root -Force; Remove-Item -LiteralPath $archive -Force }}"#);
    vec!["powershell".into(), "-NoProfile".into(), "-NonInteractive".into(), "-ExecutionPolicy".into(), "Bypass".into(), "-Command".into(), script]
}

#[cfg(not(windows))]
pub(crate) fn mysql_fallback_command() -> Vec<String> { Vec::new() }

#[cfg(not(windows))]
pub(crate) fn managed_php_dir() -> Option<std::path::PathBuf> { None }

#[cfg(not(windows))]
pub(crate) fn managed_node_dir() -> Option<std::path::PathBuf> { None }

#[cfg(not(windows))]
pub(crate) fn managed_node_present() -> bool { false }

#[cfg(not(windows))]
pub(crate) fn managed_php_present() -> bool { false }

#[cfg(not(windows))]
pub(crate) fn php_fallback_command() -> Vec<String> { Vec::new() }

#[cfg(not(windows))]
pub(crate) fn node_fallback_command() -> Vec<String> { Vec::new() }
