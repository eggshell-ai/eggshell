//! The extension points of `lint_code`.
//!
//! A check is a small, self-contained unit that inspects one aspect of a
//! generated project and returns issues. Adding a new check means adding a type
//! that implements [`Check`] and registering it in [`super::registry`] — the
//! tool itself never changes.

use std::path::{Path, PathBuf};

use crate::llm::LlmResult;

use super::issue::Issue;

/// One of the two shells every generated project has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shell {
    Backend,
    Frontend,
}

impl Shell {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Shell::Backend => "backend",
            Shell::Frontend => "frontend",
        }
    }

    /// The directory name of the shell inside a project.
    pub(crate) fn dir_name(&self) -> &'static str {
        match self {
            Shell::Backend => "backend",
            Shell::Frontend => "frontend",
        }
    }
}

/// Which shells a check knows how to inspect.
///
/// `Both` is currently unused because every check targets a single shell, but it
/// is part of the contract so a future cross-shell check needs no new plumbing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum ShellScope {
    Backend,
    Frontend,
    Both,
}

impl ShellScope {
    pub(crate) fn includes(&self, shell: Shell) -> bool {
        match (self, shell) {
            (ShellScope::Both, _) => true,
            (ShellScope::Backend, Shell::Backend) => true,
            (ShellScope::Frontend, Shell::Frontend) => true,
            _ => false,
        }
    }

    /// Shell used to attribute failures of the check itself (e.g. a crash).
    pub(crate) fn primary(&self) -> Shell {
        match self {
            ShellScope::Frontend => Shell::Frontend,
            _ => Shell::Backend,
        }
    }
}

/// Rough cost class. Static checks read files; command checks spawn a process.
/// Registry order follows this so cheap feedback arrives first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Tier {
    Static,
    Command,
}

/// Everything a check needs. Keeping this a struct (rather than loose
/// parameters) means new inputs can be added without touching every check.
pub(crate) struct CheckContext<'a> {
    pub project_path: &'a Path,
    /// Optional filter supplied by the agent (`path` argument), already
    /// validated as a safe relative path.
    pub relative_path: Option<&'a str>,
    /// Set when the caller opted into slower, command-backed checks.
    pub deep: bool,
}

impl CheckContext<'_> {
    pub(crate) fn shell_dir(&self, shell: Shell) -> PathBuf {
        self.project_path.join(shell.dir_name())
    }
}

pub(crate) trait Check: Send + Sync {
    /// Stable identifier. Also used as the issue `source`, and as the value the
    /// agent passes in `checks` / `skipChecks`.
    fn id(&self) -> &'static str;

    /// One-line description surfaced in the tool's parameter docs.
    fn description(&self) -> &'static str;

    fn shells(&self) -> ShellScope;

    fn tier(&self) -> Tier;

    /// Whether the check only runs when `deep` is enabled. Checks that spawn a
    /// Symfony console command set this so callers can keep loops fast.
    fn requires_deep(&self) -> bool {
        false
    }

    fn run(&self, ctx: &CheckContext) -> LlmResult<Vec<Issue>>;
}
