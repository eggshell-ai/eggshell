//! Frontend (JS/TS) checks.
//!
//! - [`MenuJsonCheck`] validates `src/menu-items/menu.json`.
//! - [`EslintCheck`] runs ESLint and normalises its JSON report.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Deserialize;
use serde_json::Value;

use super::check::{Check, CheckContext, Shell, ShellScope, Tier};
use super::issue::Issue;
use crate::llm::LlmResult;

pub(crate) struct MenuJsonCheck;

impl Check for MenuJsonCheck {
    fn id(&self) -> &'static str {
        "menu-json"
    }

    fn description(&self) -> &'static str {
        "Validates `src/menu-items/menu.json` parses as JSON."
    }

    fn shells(&self) -> ShellScope {
        ShellScope::Frontend
    }

    fn tier(&self) -> Tier {
        Tier::Static
    }

    fn run(&self, ctx: &CheckContext) -> LlmResult<Vec<Issue>> {
        let menu_path = resolve_frontend_dir(ctx)
            .join("src")
            .join("menu-items")
            .join("menu.json");
        if !menu_path.is_file() {
            return Ok(Vec::new());
        }

        match fs::read_to_string(&menu_path) {
            Ok(content) => match serde_json::from_str::<Value>(&content) {
                Ok(_) => Ok(Vec::new()),
                Err(error) => Ok(vec![Issue::error(
                    Shell::Frontend,
                    self.id(),
                    "src/menu-items/menu.json",
                    error.line() as u64,
                    format!("Invalid JSON: {error}"),
                )
                .with_column(Some(error.column() as u64))]),
            },
            Err(error) => Ok(vec![Issue::error(
                Shell::Frontend,
                self.id(),
                "src/menu-items/menu.json",
                0,
                format!("Failed to read src/menu-items/menu.json: {error}"),
            )]),
        }
    }
}

pub(crate) struct EslintCheck;

impl Check for EslintCheck {
    fn id(&self) -> &'static str {
        "eslint"
    }

    fn description(&self) -> &'static str {
        "ESLint over frontend `src/**/*.{js,jsx,ts,tsx}`."
    }

    fn shells(&self) -> ShellScope {
        ShellScope::Frontend
    }

    fn tier(&self) -> Tier {
        Tier::Command
    }

    fn run(&self, ctx: &CheckContext) -> LlmResult<Vec<Issue>> {
        let frontend_dir = resolve_frontend_dir(ctx);
        if !frontend_dir.is_dir() {
            return Ok(Vec::new());
        }
        lint_frontend_shell(&frontend_dir, ctx.relative_path)
    }
}

/// Picks the frontend directory, tolerating the legacy `admin-panel` name.
pub(super) fn resolve_frontend_dir(ctx: &CheckContext) -> PathBuf {
    let project = ctx.project_path;
    if project.join("frontend").is_dir() || !project.join("admin-panel").is_dir() {
        project.join("frontend")
    } else {
        project.join("admin-panel")
    }
}

#[derive(Debug, Deserialize)]
struct EslintMessage {
    line: Option<u64>,
    column: Option<u64>,
    message: Option<String>,
    #[serde(rename = "ruleId")]
    rule_id: Option<String>,
    severity: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct EslintFileResult {
    #[serde(rename = "filePath")]
    file_path: String,
    messages: Option<Vec<EslintMessage>>,
}

fn lint_frontend_shell(
    frontend_dir: &Path,
    relative_filter: Option<&str>,
) -> LlmResult<Vec<Issue>> {
    let target_pattern = if let Some(filter) = relative_filter {
        let trimmed = filter.trim_start_matches("src/").trim_start_matches("src\\");
        format!("src/{trimmed}")
    } else {
        "src/**/*.{js,jsx,ts,tsx}".to_string()
    };

    let local_eslint_bin = if cfg!(windows) {
        frontend_dir
            .join("node_modules")
            .join(".bin")
            .join("eslint.cmd")
    } else {
        frontend_dir.join("node_modules").join(".bin").join("eslint")
    };

    let mut cmd = if local_eslint_bin.is_file() {
        let mut c = Command::new(&local_eslint_bin);
        c.args([&target_pattern, "-f", "json"]);
        c
    } else if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", "npx", "eslint", &target_pattern, "-f", "json"]);
        c
    } else {
        let mut c = Command::new("npx");
        c.args(["eslint", &target_pattern, "-f", "json"]);
        c
    };

    configure_env_node(&mut cmd);
    cmd.current_dir(frontend_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    {
        super::php::hide_window(&mut cmd);
    }

    let output = cmd.output().map_err(|error| {
        format!(
            "Failed to execute ESLint in {}: {error}",
            frontend_dir.display()
        )
    })?;

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let stderr_str = String::from_utf8_lossy(&output.stderr);

    // ESLint JSON is printed to stdout. Find the array starting with '['.
    if let Some(json_start) = stdout_str.find('[') {
        let json_part = &stdout_str[json_start..];
        if let Ok(file_results) = serde_json::from_str::<Vec<EslintFileResult>>(json_part) {
            let mut issues = Vec::new();
            for file_res in file_results {
                let file_path_buf = PathBuf::from(&file_res.file_path);
                let display_path = file_path_buf
                    .strip_prefix(frontend_dir)
                    .unwrap_or(&file_path_buf)
                    .to_string_lossy()
                    .replace('\\', "/");

                // `prettier/prettier` formatting warnings (CRLF, stray commas,
                // re-indentation) are noise from scaffold files, not runtime
                // problems. Reporting thousands of them swamps the agent's
                // context, so keep only one aggregate warning per file.
                let mut prettier_count = 0usize;

                for msg in file_res.messages.unwrap_or_default() {
                    // A directive such as `// eslint-disable-next-line
                    // @next/next/no-img-element` produces an error when that rule
                    // is not registered by the active config. That is a template
                    // artifact, not a problem in the reviewed code.
                    if msg
                        .message
                        .as_deref()
                        .map(is_unknown_rule_definition)
                        .unwrap_or(false)
                    {
                        continue;
                    }

                    if let Some(ref rule) = msg.rule_id {
                        if rule == "no-unused-vars" || rule == "@typescript-eslint/no-unused-vars" {
                            continue;
                        }
                        if is_prettier_formatting_noise(rule, msg.message.as_deref()) {
                            prettier_count += 1;
                            continue;
                        }
                    }

                    let rule_desc = msg.rule_id.unwrap_or_else(|| "syntax/parser".to_string());
                    let full_message = match msg.message {
                        Some(m) => format!("[{rule_desc}] {m}"),
                        None => format!("[{rule_desc}] Lint issue"),
                    };

                    let mut issue = match msg.severity {
                        Some(1) => Issue::warning(
                            Shell::Frontend,
                            "eslint",
                            display_path.clone(),
                            msg.line.unwrap_or(1),
                            full_message,
                        ),
                        _ => Issue::error(
                            Shell::Frontend,
                            "eslint",
                            display_path.clone(),
                            msg.line.unwrap_or(1),
                            full_message,
                        ),
                    };
                    issue = issue.with_column(msg.column);
                    issues.push(issue);
                }

                if prettier_count > 0 {
                    issues.push(Issue::warning(
                        Shell::Frontend,
                        "eslint",
                        display_path.clone(),
                        1,
                        format!(
                            "[prettier/prettier] {prettier_count} formatting warning(s) in this file (line endings / spacing). Run the project's prettier to resolve; not an error."
                        ),
                    ));
                }
            }
            return Ok(issues);
        }
    }

    // ESLint exited non-zero but produced no JSON (config error or crash).
    if !output.status.success() {
        let raw_err_text = if !stderr_str.trim().is_empty() {
            stderr_str.trim()
        } else if !stdout_str.trim().is_empty() {
            stdout_str.trim()
        } else {
            "ESLint failed with unknown error"
        };

        let err_text = raw_err_text
            .lines()
            .filter(|line| !is_unknown_rule_definition(line))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();

        if !err_text.is_empty() {
            return Ok(vec![Issue::error(
                Shell::Frontend,
                "eslint",
                relative_filter.unwrap_or("src"),
                1,
                err_text,
            )]);
        }
    }

    Ok(Vec::new())
}

/// ESLint reports `Definition for rule '<name>' was not found.` (and variants)
/// when an inline disable directive references a rule the active flat config
/// does not register. This is template configuration, not a source error.
fn is_unknown_rule_definition(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();

    (lower.contains("definition for rule") && lower.contains("was not found"))
        || lower.contains("unknown rule")
        || (lower.contains("couldn't find rule") && lower.contains("definition"))
}

/// Prettier messages are pure formatting fixes (the template ships files with
/// CRLF and stray commas while its `.prettierrc` demands LF). They can number
/// in the thousands and carry zero semantic information, so each one is
/// collapsed into a per-file aggregate instead of being reported line by line.
fn is_prettier_formatting_noise(rule: &str, message: Option<&str>) -> bool {
    if rule != "prettier/prettier" {
        return false;
    }

    let Some(message) = message else {
        return true;
    };
    message.starts_with("Delete ") || message.starts_with("Insert ") || message.starts_with("Replace ")
}

fn configure_env_node(command: &mut Command) {
    command.env_clear();
    for name in [
        "PATH",
        "HOME",
        "APPDATA",
        "USERPROFILE",
        "SystemRoot",
        "ComSpec",
        "PATHEXT",
        "TEMP",
        "TMP",
        "LOCALAPPDATA",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }

    #[cfg(windows)]
    if let Some(directory) = crate::setup::managed_node_dir() {
        if directory.join("node.exe").is_file() {
            crate::llm::symfony::prepend_to_path(command, &directory);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prettier_formatting_noise_detection() {
        assert!(is_prettier_formatting_noise("prettier/prettier", Some("Delete `␍`")));
        assert!(is_prettier_formatting_noise(
            "prettier/prettier",
            Some("Delete `,`")
        ));
        assert!(is_prettier_formatting_noise(
            "prettier/prettier",
            Some("Insert `⏎`")
        ));
        assert!(is_prettier_formatting_noise(
            "prettier/prettier",
            Some("Replace `····` with `·····`")
        ));
        assert!(is_prettier_formatting_noise("prettier/prettier", None));

        // Prettier rule but not a formatting fix => reported normally.
        assert!(!is_prettier_formatting_noise(
            "prettier/prettier",
            Some("Code style issues found. Run Prettier to fix.")
        ));
        // Other rules are never treated as formatting noise.
        assert!(!is_prettier_formatting_noise(
            "react-hooks/exhaustive-deps",
            Some("Delete `␍`")
        ));
    }

    #[test]
    fn test_unknown_rule_definition_filtering() {
        assert!(is_unknown_rule_definition(
            "Definition for rule '@next/next/no-img-element' was not found."
        ));
        assert!(is_unknown_rule_definition(
            "unknown rule '@next/next/no-img-element'"
        ));

        assert!(!is_unknown_rule_definition(
            "Use of eval in 'data' is strongly discouraged"
        ));
        assert!(!is_unknown_rule_definition("Parsing error: Unexpected token"));
    }

    #[test]
    fn menu_json_check_reports_invalid_json() {
        let root = std::env::temp_dir().join("eggshell_lint_menu_json_check");
        let _ = fs::remove_dir_all(&root);
        let menu_dir = root.join("frontend").join("src").join("menu-items");
        fs::create_dir_all(&menu_dir).unwrap();
        fs::write(
            menu_dir.join("menu.json"),
            "[\n  {\n    \"name\": \"Test\"\n  ,\n]",
        )
        .unwrap();

        let ctx = CheckContext {
            project_path: &root,
            relative_path: None,
            deep: false,
        };
        let issues = MenuJsonCheck.run(&ctx).unwrap();
        assert_eq!(issues.len(), 1);
        assert!(issues[0].is_error());
        assert_eq!(issues[0].source, "menu-json");
        assert!(issues[0].line > 0);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn menu_json_check_accepts_valid_json() {
        let root = std::env::temp_dir().join("eggshell_lint_menu_json_valid");
        let _ = fs::remove_dir_all(&root);
        let menu_dir = root.join("frontend").join("src").join("menu-items");
        fs::create_dir_all(&menu_dir).unwrap();
        fs::write(menu_dir.join("menu.json"), "[]").unwrap();

        let ctx = CheckContext {
            project_path: &root,
            relative_path: None,
            deep: false,
        };
        assert!(MenuJsonCheck.run(&ctx).unwrap().is_empty());

        let _ = fs::remove_dir_all(&root);
    }
}
