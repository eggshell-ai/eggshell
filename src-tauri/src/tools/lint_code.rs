use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

use crate::llm::{LlmResult, Tool};

pub struct LintCodeTool {
    parameters: Map<String, Value>,
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
#[allow(dead_code)]
struct EslintFileResult {
    #[serde(rename = "filePath")]
    file_path: String,
    messages: Option<Vec<EslintMessage>>,
    #[serde(rename = "errorCount")]
    error_count: Option<u64>,
    #[serde(rename = "warningCount")]
    warning_count: Option<u64>,
}

impl LintCodeTool {
    pub fn new() -> Self {
        Self {
            parameters: json!({
                "type": "object",
                "properties": {
                    "shell": {
                        "type": "string",
                        "enum": ["frontend", "backend", "all"],
                        "description": "Target shell to lint: 'frontend' (JS/TS via ESLint), 'backend' (PHP syntax lint), or 'all' (both)."
                    },
                    "path": {
                        "type": "string",
                        "description": "Optional relative file or folder path within the shell's src directory to lint. If omitted, lints all src files."
                    },
                    "projectPath": {
                        "type": "string",
                        "description": "Overrides the target project directory."
                    }
                },
                "required": ["shell"]
            })
            .as_object()
            .expect("lint_code parameters must be an object")
            .clone(),
        }
    }
}

impl Default for LintCodeTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for LintCodeTool {
    fn name(&self) -> &str {
        "lint_code"
    }

    fn description(&self) -> &str {
        "Lints PHP (backend) and JavaScript/TypeScript (frontend) code for an AdminPanel project, reporting any syntax and lint errors with exact file and line locations."
    }

    fn parameters(&self) -> &Map<String, Value> {
        &self.parameters
    }

    async fn execute(&self, args: Value) -> LlmResult<Value> {
        let shell = args
            .get("shell")
            .and_then(Value::as_str)
            .ok_or_else(|| "shell is required (must be 'frontend', 'backend', or 'all')".to_string())?;

        if !matches!(shell, "frontend" | "backend" | "all") {
            return Err("shell must be 'frontend', 'backend', or 'all'".into());
        }

        let relative_path = args
            .get("path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|p| !p.is_empty());

        if let Some(rel) = relative_path {
            let p = Path::new(rel);
            if p.is_absolute()
                || p.components().any(|c| {
                    matches!(
                        c,
                        Component::ParentDir | Component::RootDir | Component::Prefix(_)
                    )
                })
            {
                return Err("path must be a safe relative path".into());
            }
        }

        let project_path = args
            .get("projectPath")
            .and_then(Value::as_str)
            .filter(|p| !p.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("content").join("dummy-project"));

        let mut issues: Vec<Value> = Vec::new();
        let mut total_errors: usize = 0;
        let mut total_warnings: usize = 0;

        if shell == "backend" || shell == "all" {
            let backend_dir = project_path.join("backend");
            if backend_dir.is_dir() {
                let php_issues = lint_php_shell(&backend_dir, relative_path)?;
                for issue in php_issues {
                    if issue.get("severity").and_then(Value::as_str) == Some("warning") {
                        total_warnings += 1;
                    } else {
                        total_errors += 1;
                    }
                    issues.push(issue);
                }
            } else if shell == "backend" {
                return Ok(json!({
                    "success": false,
                    "message": format!("Backend directory does not exist at {}", backend_dir.display()),
                    "totalErrors": 1,
                    "totalWarnings": 0,
                    "issues": [{
                        "shell": "backend",
                        "file": "backend",
                        "line": 0,
                        "message": "Backend directory not found",
                        "severity": "error",
                        "source": "fs"
                    }]
                }));
            }
        }

        if shell == "frontend" || shell == "all" {
            // Frontend shell directory is usually "frontend" (or "admin-panel" fallback)
            let frontend_dir = if project_path.join("frontend").is_dir() {
                project_path.join("frontend")
            } else if project_path.join("admin-panel").is_dir() {
                project_path.join("admin-panel")
            } else {
                project_path.join("frontend")
            };

            if frontend_dir.is_dir() {
                let js_issues = lint_frontend_shell(&frontend_dir, relative_path)?;
                for issue in js_issues {
                    if issue.get("severity").and_then(Value::as_str) == Some("warning") {
                        total_warnings += 1;
                    } else {
                        total_errors += 1;
                    }
                    issues.push(issue);
                }
            } else if shell == "frontend" {
                return Ok(json!({
                    "success": false,
                    "message": format!("Frontend directory does not exist at {}", frontend_dir.display()),
                    "totalErrors": 1,
                    "totalWarnings": 0,
                    "issues": [{
                        "shell": "frontend",
                        "file": "frontend",
                        "line": 0,
                        "message": "Frontend directory not found",
                        "severity": "error",
                        "source": "fs"
                    }]
                }));
            }
        }

        let success = total_errors == 0;
        let message = if success {
            if total_warnings > 0 {
                format!("Lint completed successfully with {total_warnings} warning(s) and 0 errors.")
            } else {
                "Lint passed successfully with 0 errors and 0 warnings.".to_string()
            }
        } else {
            format!("Lint found {total_errors} error(s) and {total_warnings} warning(s).")
        };

        Ok(json!({
            "success": success,
            "message": message,
            "totalErrors": total_errors,
            "totalWarnings": total_warnings,
            "issues": issues
        }))
    }
}

// -----------------------------------------------------------------------------
// PHP Linting Implementation (via php -l)
// -----------------------------------------------------------------------------

fn lint_php_shell(backend_dir: &Path, relative_filter: Option<&str>) -> LlmResult<Vec<Value>> {
    let mut issues = Vec::new();
    let src_dir = backend_dir.join("src");

    let mut target_files: Vec<PathBuf> = Vec::new();

    if let Some(filter) = relative_filter {
        let p1 = src_dir.join(filter);
        let p2 = backend_dir.join(filter);
        let candidate = if p1.exists() {
            p1
        } else if p2.exists() {
            p2
        } else {
            p1
        };

        if candidate.is_file() {
            if candidate.extension().and_then(|s| s.to_str()) == Some("php") {
                target_files.push(candidate);
            }
        } else if candidate.is_dir() {
            collect_php_files(&candidate, &mut target_files);
        } else {
            issues.push(json!({
                "shell": "backend",
                "file": filter,
                "line": 0,
                "column": null,
                "message": format!("Target file or folder not found: {}", candidate.display()),
                "severity": "error",
                "source": "php -l"
            }));
            return Ok(issues);
        }
    } else {
        if src_dir.is_dir() {
            collect_php_files(&src_dir, &mut target_files);
        }
        // Also check config and migrations if they exist
        let config_dir = backend_dir.join("config");
        if config_dir.is_dir() {
            collect_php_files(&config_dir, &mut target_files);
        }
        let public_index = backend_dir.join("public").join("index.php");
        if public_index.is_file() {
            target_files.push(public_index);
        }
    }

    for file_path in target_files {
        let display_path = file_path
            .strip_prefix(backend_dir)
            .unwrap_or(&file_path)
            .to_string_lossy()
            .replace('\\', "/");

        let mut cmd = Command::new("php");
        configure_env_php(&mut cmd);
        cmd.arg("-l")
            .arg(&file_path)
            .current_dir(backend_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let output = cmd.output().map_err(|e| {
            format!(
                "Failed to execute php -l on {}: {e}",
                file_path.display()
            )
        })?;

        if !output.status.success() {
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
                    // Extract line number
                    if let Some(on_line_idx) = trimmed.rfind(" on line ") {
                        let line_str = &trimmed[on_line_idx + 9..].trim_end_matches('.');
                        if let Ok(l) = line_str.parse::<u64>() {
                            found_line = Some(l);
                        }
                    }

                    // Extract message
                    let msg_part = trimmed
                        .trim_start_matches("PHP Parse error:")
                        .trim_start_matches("Parse error:")
                        .trim();

                    if let Some(in_idx) = msg_part.find(" in ") {
                        error_msg = msg_part[..in_idx].trim().to_string();
                    } else {
                        error_msg = msg_part.to_string();
                    }
                }
            }

            if error_msg.is_empty() {
                error_msg = combined_output.trim().to_string();
            }

            issues.push(json!({
                "shell": "backend",
                "file": display_path,
                "line": found_line.unwrap_or(1),
                "column": null,
                "message": error_msg,
                "severity": "error",
                "source": "php -l"
            }));
        }
    }

    // Run Doctrine schema validation if bin/console and vendor exist
    let console_bin = backend_dir.join("bin").join("console");
    let vendor_dir = backend_dir.join("vendor");
    if console_bin.is_file() && vendor_dir.is_dir() {
        let mut cmd = Command::new("php");
        configure_env_php(&mut cmd);
        cmd.args(["bin/console", "doctrine:schema:validate", "--skip-sync", "--no-interaction"])
            .current_dir(backend_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        if let Ok(output) = cmd.output() {
            if !output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let combined = format!("{}\n{}", stdout, stderr);

                // Collect any mapping error lines or output
                let mut mapping_errors = Vec::new();
                for line in combined.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("[FAIL]") || trimmed.starts_with("[ERROR]") || trimmed.contains("The mapping files") || trimmed.contains("Exception") {
                        mapping_errors.push(trimmed);
                    }
                }

                let error_message = if !mapping_errors.is_empty() {
                    mapping_errors.join("\n")
                } else {
                    combined.trim().to_string()
                };

                issues.push(json!({
                    "shell": "backend",
                    "file": "src/Entity",
                    "line": 1,
                    "column": null,
                    "message": format!("Doctrine schema validation failed: {}", error_message),
                    "severity": "error",
                    "source": "doctrine:schema:validate"
                }));
            }
        }
    }

    Ok(issues)
}

fn collect_php_files(dir: &Path, list: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = entry.file_name();
            let name_str = file_name.to_string_lossy();
            if path.is_dir() {
                if !["vendor", ".git", "var", "cache"].contains(&name_str.as_ref()) {
                    collect_php_files(&path, list);
                }
            } else if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("php") {
                list.push(path);
            }
        }
    }
}

fn configure_env_php(command: &mut Command) {
    command.env_clear();
    for name in [
        "PATH",
        "HOME",
        "APPDATA",
        "COMPOSER_HOME",
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
    if let Some(directory) = crate::setup::managed_php_dir() {
        prepend_to_path(command, &directory);
    }
}

// -----------------------------------------------------------------------------
// JavaScript / TypeScript Linting Implementation (via ESLint)
// -----------------------------------------------------------------------------

fn lint_frontend_shell(frontend_dir: &Path, relative_filter: Option<&str>) -> LlmResult<Vec<Value>> {
    let mut issues = Vec::new();

    let target_pattern = if let Some(filter) = relative_filter {
        let trimmed = filter.trim_start_matches("src/").trim_start_matches("src\\");
        format!("src/{trimmed}")
    } else {
        "src/**/*.{js,jsx,ts,tsx}".to_string()
    };

    // Try finding local eslint binary first
    let local_eslint_bin = if cfg!(windows) {
        frontend_dir.join("node_modules").join(".bin").join("eslint.cmd")
    } else {
        frontend_dir.join("node_modules").join(".bin").join("eslint")
    };

    let mut cmd = if local_eslint_bin.is_file() {
        let mut c = Command::new(&local_eslint_bin);
        c.args([&target_pattern, "-f", "json"]);
        c
    } else {
        // Fallback to npx eslint
        if cfg!(windows) {
            let mut c = Command::new("cmd");
            c.args(["/C", "npx", "eslint", &target_pattern, "-f", "json"]);
            c
        } else {
            let mut c = Command::new("npx");
            c.args(["eslint", &target_pattern, "-f", "json"]);
            c
        }
    };

    configure_env_node(&mut cmd);
    cmd.current_dir(frontend_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let output = cmd.output().map_err(|e| {
        format!(
            "Failed to execute ESLint in {}: {e}",
            frontend_dir.display()
        )
    })?;

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let stderr_str = String::from_utf8_lossy(&output.stderr);

    // ESLint JSON is printed to stdout. Find json array starting with '['
    if let Some(json_start) = stdout_str.find('[') {
        let json_part = &stdout_str[json_start..];
        if let Ok(file_results) = serde_json::from_str::<Vec<EslintFileResult>>(json_part) {
            for file_res in file_results {
                let file_path_buf = PathBuf::from(&file_res.file_path);
                let display_path = file_path_buf
                    .strip_prefix(frontend_dir)
                    .unwrap_or(&file_path_buf)
                    .to_string_lossy()
                    .replace('\\', "/");

                if let Some(messages) = file_res.messages {
                    for msg in messages {
                        let severity_str = match msg.severity {
                            Some(1) => "warning",
                            _ => "error",
                        };

                        let rule_desc = msg.rule_id.unwrap_or_else(|| "syntax/parser".to_string());
                        let full_message = match msg.message {
                            Some(m) => format!("[{rule_desc}] {m}"),
                            None => format!("[{rule_desc}] Lint issue"),
                        };

                        issues.push(json!({
                            "shell": "frontend",
                            "file": display_path,
                            "line": msg.line.unwrap_or(1),
                            "column": msg.column,
                            "message": full_message,
                            "severity": severity_str,
                            "source": "eslint"
                        }));
                    }
                }
            }
            return Ok(issues);
        }
    }

    // If ESLint exited non-zero but couldn't produce JSON output (e.g. config error or crash)
    if !output.status.success() {
        let err_text = if !stderr_str.trim().is_empty() {
            stderr_str.trim()
        } else if !stdout_str.trim().is_empty() {
            stdout_str.trim()
        } else {
            "ESLint failed with unknown error"
        };

        issues.push(json!({
            "shell": "frontend",
            "file": relative_filter.unwrap_or("src"),
            "line": 1,
            "column": null,
            "message": err_text,
            "severity": "error",
            "source": "eslint"
        }));
    }

    Ok(issues)
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
            prepend_to_path(command, &directory);
        }
    }
}

fn prepend_to_path(command: &mut Command, directory: &Path) {
    let mut value = directory.as_os_str().to_os_string();
    if let Some(existing) = std::env::var_os("PATH").filter(|existing| !existing.is_empty()) {
        value.push(if cfg!(windows) { ";" } else { ":" });
        value.push(existing);
    }
    command.env("PATH", value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name_and_params() {
        let tool = LintCodeTool::new();
        assert_eq!(tool.name(), "lint_code");
        assert!(tool.parameters().contains_key("properties"));
    }

    #[tokio::test]
    async fn test_backend_lint_on_clean_template() {
        let tool = LintCodeTool::new();
        let templates_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("templates");

        // templates dir has "backend"
        let args = json!({
            "shell": "backend",
            "projectPath": templates_dir.to_str().unwrap()
        });

        let res = tool.execute(args).await.expect("Tool execution failed");
        assert_eq!(res["success"], true);
        assert_eq!(res["totalErrors"], 0);
    }

    #[tokio::test]
    async fn test_backend_lint_detects_syntax_error() {
        let tool = LintCodeTool::new();
        let temp_dir = std::env::temp_dir().join("eggshell_lint_test");
        let backend_src = temp_dir.join("backend").join("src");
        fs::create_dir_all(&backend_src).unwrap();

        let bad_file = backend_src.join("BadSyntax.php");
        fs::write(&bad_file, "<?php\n\nfunction broken() {\n    echo 1 +\n}\n").unwrap();

        let args = json!({
            "shell": "backend",
            "projectPath": temp_dir.to_str().unwrap()
        });

        let res = tool.execute(args).await.expect("Tool execution failed");
        assert_eq!(res["success"], false);
        assert!(res["totalErrors"].as_u64().unwrap() >= 1);

        let issues = res["issues"].as_array().unwrap();
        let bad_issue = issues
            .iter()
            .find(|i| i["file"].as_str().unwrap().contains("BadSyntax.php"));
        assert!(bad_issue.is_some());
        let bad = bad_issue.unwrap();
        assert_eq!(bad["line"], 5); // line 5 is where '}' is unexpected after '1 +'

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }
}

