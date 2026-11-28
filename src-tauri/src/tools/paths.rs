//! Shared path resolution for the filesystem tools.
//!
//! Every file tool operates on a *shell* (`frontend` or `backend`) and a path
//! that is relative to that shell's `src/` directory. Absolute paths, `..`
//! traversal, root components and Windows prefixes are rejected. The project
//! directory comes from the optional `projectPath` argument, which the agent
//! loop injects from the selected project, so a tool never silently falls back
//! to the app's own working directory.

use serde_json::Value;
use std::path::{Component, Path, PathBuf};

use crate::llm::LlmResult;

/// A resolved location inside a generated project.
#[derive(Debug)]
pub(crate) struct ShellPath {
    pub shell: String,
    /// The caller-supplied path, trimmed. Empty means the shell's `src/` root.
    pub relative: String,
    /// `root` joined with `relative`.
    pub target: PathBuf,
}

/// Resolves the `shell` + `path` + `projectPath` arguments shared by the file
/// tools.
///
/// `allow_root` lets a tool treat an empty (or missing) `path` as the shell's
/// `src/` directory itself, which is what `list_directory` needs.
pub(crate) fn resolve_shell_path(args: &Value, allow_root: bool) -> LlmResult<ShellPath> {
    let shell = args
        .get("shell")
        .and_then(Value::as_str)
        .ok_or_else(|| "shell is required".to_string())?;
    if !matches!(shell, "frontend" | "backend") {
        return Err("shell must be either 'frontend' or 'backend'".into());
    }

    let raw = args
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");

    if raw.is_empty() && !allow_root {
        return Err("path is required and must be a non-empty relative path"
            .to_string()
            .into());
    }

    let relative = Path::new(raw);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err("path must remain within the shell src directory".into());
    }

    let project_path = args
        .get("projectPath")
        .and_then(Value::as_str)
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("content").join("dummy-project"));

    let root = project_path.join(shell).join("src");
    let target = if raw.is_empty() {
        root.clone()
    } else {
        root.join(relative)
    };

    Ok(ShellPath {
        shell: shell.to_string(),
        relative: raw.replace('\\', "/"),
        target,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn resolves_relative_paths_under_the_shell_src() {
        let resolved = resolve_shell_path(
            &json!({ "shell": "frontend", "path": "app/(dashboard)/page.tsx", "projectPath": "/tmp/p" }),
            false,
        )
        .expect("path resolves");
        assert_eq!(resolved.shell, "frontend");
        assert_eq!(resolved.relative, "app/(dashboard)/page.tsx");
        assert!(resolved.target.ends_with("frontend/src/app/(dashboard)/page.tsx"));
    }

    #[test]
    fn rejects_traversal_and_absolute_paths() {
        for bad in ["../secrets", "/etc/passwd", "a/../../b"] {
            let error = resolve_shell_path(&json!({ "shell": "backend", "path": bad }), true)
                .expect_err("path must be rejected");
            assert!(error.to_string().contains("must remain within"));
        }
    }

    #[test]
    fn empty_path_is_only_allowed_when_requested() {
        assert!(resolve_shell_path(&json!({ "shell": "backend" }), false).is_err());
        let resolved = resolve_shell_path(&json!({ "shell": "backend" }), true).expect("root ok");
        assert!(resolved.target.ends_with("backend/src"));
        assert!(resolved.relative.is_empty());
    }

    #[test]
    fn rejects_unknown_shells() {
        let error = resolve_shell_path(&json!({ "shell": "mobile", "path": "x" }), false)
            .expect_err("shell must be validated");
        assert!(error.to_string().contains("frontend"));
    }
}
