//! The single place that defines what a lint issue looks like and how it is
//! serialised for the agent.
//!
//! Every check reports through [`Issue`] so that adding a new check never
//! requires touching the tool's output contract. The JSON keys below are part of
//! the agent-facing API and must stay stable.

use serde_json::{json, Value};

use super::check::Shell;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Issue {
    pub shell: Shell,
    /// Path relative to the shell directory, always with forward slashes.
    pub file: String,
    /// 1-based line number. `0`/`1` is used when the check reports a file-level
    /// problem.
    pub line: u64,
    pub column: Option<u64>,
    pub message: String,
    pub severity: Severity,
    /// The check that produced this issue; surfaces as `source` in the JSON.
    pub source: String,
}

impl Issue {
    pub(crate) fn error(
        shell: Shell,
        source: impl Into<String>,
        file: impl Into<String>,
        line: u64,
        message: impl Into<String>,
    ) -> Self {
        Self {
            shell,
            file: file.into(),
            line,
            column: None,
            message: message.into(),
            severity: Severity::Error,
            source: source.into(),
        }
    }

    pub(crate) fn warning(
        shell: Shell,
        source: impl Into<String>,
        file: impl Into<String>,
        line: u64,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: Severity::Warning,
            ..Self::error(shell, source, file, line, message)
        }
    }

    pub(crate) fn with_column(mut self, column: Option<u64>) -> Self {
        self.column = column;
        self
    }

    pub(crate) fn is_error(&self) -> bool {
        matches!(self.severity, Severity::Error)
    }

    pub(crate) fn to_json(&self) -> Value {
        json!({
            "shell": self.shell.as_str(),
            "file": self.file,
            "line": self.line,
            "column": self.column,
            "message": self.message,
            "severity": self.severity.as_str(),
            "source": self.source,
        })
    }
}
