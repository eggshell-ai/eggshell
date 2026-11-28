//! Backend (PHP) checks.
//!
//! - [`PhpSyntaxCheck`] runs `php -l` over the shell's PHP files.
//! - [`DoctrineSchemaCheck`] runs `doctrine:schema:validate`.
//! - [`AnalyticsReportsCheck`] runs the project's `app:analytics:test` harness.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde_json::Value;

use super::check::{Check, CheckContext, Shell, ShellScope, Tier};
use super::issue::Issue;
use crate::llm::LlmResult;

/// Directories that hold no lintable application code.
const SKIPPED_DIRS: [&str; 5] = ["vendor", ".git", "var", "cache", "node_modules"];

pub(crate) struct PhpSyntaxCheck;

impl Check for PhpSyntaxCheck {
    fn id(&self) -> &'static str {
        "php-syntax"
    }

    fn description(&self) -> &'static str {
        "PHP syntax via `php -l` for backend src/config files."
    }

    fn shells(&self) -> ShellScope {
        ShellScope::Backend
    }

    fn tier(&self) -> Tier {
        Tier::Command
    }

    fn run(&self, ctx: &CheckContext) -> LlmResult<Vec<Issue>> {
        let backend_dir = ctx.shell_dir(Shell::Backend);
        if !backend_dir.is_dir() {
            return Ok(Vec::new());
        }

        let mut issues = Vec::new();
        let target_files = php_target_files(&backend_dir, ctx.relative_path);

        if target_files.is_none() {
            issues.push(Issue::error(
                Shell::Backend,
                self.id(),
                ctx.relative_path.unwrap_or("src"),
                0,
                format!(
                    "Target file or folder not found: {}",
                    backend_dir.join(ctx.relative_path.unwrap_or_default()).display()
                ),
            ));
            return Ok(issues);
        }

        for file_path in target_files.unwrap_or_default() {
            if let Some(issue) = lint_php_file(&backend_dir, &file_path)? {
                issues.push(issue);
            }
        }

        Ok(issues)
    }
}

pub(crate) struct DoctrineSchemaCheck;

impl Check for DoctrineSchemaCheck {
    fn id(&self) -> &'static str {
        "doctrine-schema"
    }

    fn description(&self) -> &'static str {
        "Doctrine mapping validation (`doctrine:schema:validate --skip-sync`)."
    }

    fn shells(&self) -> ShellScope {
        ShellScope::Backend
    }

    fn tier(&self) -> Tier {
        Tier::Command
    }

    fn requires_deep(&self) -> bool {
        true
    }

    fn run(&self, ctx: &CheckContext) -> LlmResult<Vec<Issue>> {
        let backend_dir = ctx.shell_dir(Shell::Backend);
        if !console_ready(&backend_dir) {
            return Ok(Vec::new());
        }

        let mut cmd = crate::llm::symfony::new_php_command();
        cmd.args([
            "bin/console",
            "doctrine:schema:validate",
            "--skip-sync",
            "--no-interaction",
        ])
        .current_dir(&backend_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        hide_window(&mut cmd);

        let output = match cmd.output() {
            Ok(output) => output,
            Err(_) => return Ok(Vec::new()),
        };
        if output.status.success() {
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{stdout}\n{stderr}");

        let mut mapping_errors = Vec::new();
        for line in combined.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("[FAIL]")
                || trimmed.starts_with("[ERROR]")
                || trimmed.contains("The mapping files")
                || trimmed.contains("Exception")
            {
                mapping_errors.push(trimmed);
            }
        }

        let error_message = if mapping_errors.is_empty() {
            combined.trim().to_string()
        } else {
            mapping_errors.join("\n")
        };

        Ok(vec![Issue::error(
            Shell::Backend,
            self.id(),
            "src/Entity",
            1,
            format!("Doctrine schema validation failed: {error_message}"),
        )])
    }
}

pub(crate) struct AnalyticsReportsCheck;

impl Check for AnalyticsReportsCheck {
    fn id(&self) -> &'static str {
        "analytics-reports"
    }

    fn description(&self) -> &'static str {
        "Runs the project's `app:analytics:test` harness, when present."
    }

    fn shells(&self) -> ShellScope {
        ShellScope::Backend
    }

    fn tier(&self) -> Tier {
        Tier::Command
    }

    fn requires_deep(&self) -> bool {
        true
    }

    fn run(&self, ctx: &CheckContext) -> LlmResult<Vec<Issue>> {
        let backend_dir = ctx.shell_dir(Shell::Backend);
        let test_reports_cmd = backend_dir
            .join("src")
            .join("Command")
            .join("TestReportsCommand.php");
        if !console_ready(&backend_dir) || !test_reports_cmd.is_file() {
            return Ok(Vec::new());
        }

        let mut cmd = crate::llm::symfony::new_php_command();
        cmd.args(["bin/console", "app:analytics:test", "--json", "--no-interaction"])
            .current_dir(&backend_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        hide_window(&mut cmd);

        let mut issues = Vec::new();
        let command_issue = |message: String| {
            Issue::error(
                Shell::Backend,
                "analytics-reports",
                "src/Command/TestReportsCommand.php",
                1,
                message,
            )
        };

        let output = match cmd.output() {
            Err(error) => {
                issues.push(command_issue(format!(
                    "Failed to run analytics test command (app:analytics:test): {error}"
                )));
                return Ok(issues);
            }
            Ok(output) => output,
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let trimmed = stdout.trim();

        // PHP warnings/deprecations may precede the JSON payload, so fall back to
        // parsing from the first '{'.
        let parsed = serde_json::from_str::<Value>(trimmed).ok().or_else(|| {
            trimmed
                .find('{')
                .and_then(|i| serde_json::from_str::<Value>(&trimmed[i..]).ok())
        });

        match parsed
            .as_ref()
            .and_then(|p| p.get("issues"))
            .and_then(Value::as_array)
        {
            Some(found) => {
                for issue in found {
                    let file = issue
                        .get("file")
                        .and_then(Value::as_str)
                        .unwrap_or("src/Service/Analytics");
                    let line = issue.get("line").and_then(Value::as_u64).unwrap_or(1);
                    let err_msg = issue.get("message").and_then(Value::as_str).unwrap_or(
                        "Analytics aggregator failed during test execution",
                    );

                    issues.push(Issue::error(
                        Shell::Backend,
                        self.id(),
                        file,
                        line,
                        err_msg,
                    ));
                }

                // Command failed but reported no issues: don't swallow it.
                if found.is_empty() && !output.status.success() {
                    issues.push(command_issue(format!(
                        "Analytics test command failed: {}\n{}",
                        trimmed,
                        stderr.trim()
                    )));
                }
            }
            None => {
                // No parseable report payload: the command did not work.
                let detail = format!("{}\n{}", trimmed, stderr.trim());
                let prefix = if output.status.success() {
                    "Analytics test command produced unparseable output"
                } else {
                    "Analytics test command failed"
                };
                issues.push(command_issue(format!("{prefix}: {}", detail.trim())));
            }
        }

        Ok(issues)
    }
}

// -----------------------------------------------------------------------------
// Shared helpers
// -----------------------------------------------------------------------------

/// `bin/console` plus installed dependencies are required before any console
/// command can run.
pub(super) fn console_ready(backend_dir: &Path) -> bool {
    backend_dir.join("bin").join("console").is_file() && backend_dir.join("vendor").is_dir()
}

#[cfg(windows)]
pub(super) fn hide_window(cmd: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
}

#[cfg(not(windows))]
pub(super) fn hide_window(_cmd: &mut std::process::Command) {}

/// Resolves the PHP files to lint.
///
/// `None` means the agent-supplied filter matched nothing; `Some(files)` is the
/// (possibly empty) list to check.
fn php_target_files(backend_dir: &Path, relative_filter: Option<&str>) -> Option<Vec<PathBuf>> {
    let mut target_files: Vec<PathBuf> = Vec::new();
    let src_dir = backend_dir.join("src");

    if let Some(filter) = relative_filter {
        let prefixed = src_dir.join(filter);
        let rooted = backend_dir.join(filter);
        let candidate = if prefixed.exists() {
            prefixed
        } else if rooted.exists() {
            rooted
        } else {
            return None;
        };

        if candidate.is_file() {
            if candidate.extension().and_then(|s| s.to_str()) == Some("php") {
                target_files.push(candidate);
            }
        } else if candidate.is_dir() {
            collect_php_files(&candidate, &mut target_files);
        }

        return Some(target_files);
    }

    if src_dir.is_dir() {
        collect_php_files(&src_dir, &mut target_files);
    }
    let config_dir = backend_dir.join("config");
    if config_dir.is_dir() {
        collect_php_files(&config_dir, &mut target_files);
    }
    let public_index = backend_dir.join("public").join("index.php");
    if public_index.is_file() {
        target_files.push(public_index);
    }

    Some(target_files)
}

pub(super) fn collect_php_files(dir: &Path, list: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = entry.file_name();
            let name_str = file_name.to_string_lossy();
            if path.is_dir() {
                if !SKIPPED_DIRS.contains(&name_str.as_ref()) {
                    collect_php_files(&path, list);
                }
            } else if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("php") {
                list.push(path);
            }
        }
    }
}

fn lint_php_file(backend_dir: &Path, file_path: &Path) -> LlmResult<Option<Issue>> {
    let display_path = file_path
        .strip_prefix(backend_dir)
        .unwrap_or(file_path)
        .to_string_lossy()
        .replace('\\', "/");

    let mut cmd = crate::llm::symfony::new_php_command();
    cmd.arg("-l")
        .arg(file_path)
        .current_dir(backend_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_window(&mut cmd);

    let output = cmd.output().map_err(|error| {
        format!(
            "Failed to execute php -l on {}: {error}",
            file_path.display()
        )
    })?;

    if output.status.success() {
        return Ok(None);
    }

    let combined_output = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Parse PHP syntax error: "PHP Parse error:  <error> in <file> on line <line>"
    let mut found_line = None;
    let mut error_msg = String::new();

    for line in combined_output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("PHP Parse error:") || trimmed.starts_with("Parse error:") {
            if let Some(on_line_idx) = trimmed.rfind(" on line ") {
                let line_str = trimmed[on_line_idx + 9..].trim_end_matches('.');
                if let Ok(parsed) = line_str.parse::<u64>() {
                    found_line = Some(parsed);
                }
            }

            let msg_part = trimmed
                .trim_start_matches("PHP Parse error:")
                .trim_start_matches("Parse error:")
                .trim();

            error_msg = match msg_part.find(" in ") {
                Some(in_idx) => msg_part[..in_idx].trim().to_string(),
                None => msg_part.to_string(),
            };
        }
    }

    if error_msg.is_empty() {
        error_msg = combined_output.trim().to_string();
    }

    Ok(Some(Issue::error(
        Shell::Backend,
        "php-syntax",
        display_path,
        found_line.unwrap_or(1),
        error_msg,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_php_files_but_skips_vendor_and_var() {
        let root = std::env::temp_dir().join("eggshell_lint_php_files");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src").join("Controller")).unwrap();
        fs::create_dir_all(root.join("vendor")).unwrap();
        fs::create_dir_all(root.join("var")).unwrap();

        fs::write(root.join("src").join("Controller").join("Foo.php"), "<?php").unwrap();
        fs::write(root.join("vendor").join("Bar.php"), "<?php").unwrap();
        fs::write(root.join("var").join("Baz.php"), "<?php").unwrap();

        let mut files = Vec::new();
        collect_php_files(&root.join("src"), &mut files);
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("Foo.php"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_filter_reports_none() {
        let root = std::env::temp_dir().join("eggshell_lint_php_missing_filter");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();

        assert!(php_target_files(&root, Some("Nope.php")).is_none());
        assert!(php_target_files(&root, None).is_some());

        let _ = fs::remove_dir_all(&root);
    }
}
