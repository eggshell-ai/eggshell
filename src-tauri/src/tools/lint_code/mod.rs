//! `lint_code` — a pluggable set of checks over a generated project.
//!
//! The tool validates its arguments, picks the checks that apply to the requested
//! shell, and aggregates their issues. Everything a check knows how to inspect
//! lives behind the [`Check`](check::Check) trait, so new validations are added
//! by registering them in [`registry`] rather than by editing this file.
//!
//! Output contract (stable, relied on by the agent prompts):
//!
//! ```json
//! {
//!   "success": bool,
//!   "message": string,
//!   "totalErrors": number,
//!   "totalWarnings": number,
//!   "issues": [{ "shell", "file", "line", "column", "message", "severity", "source" }]
//! }
//! ```

mod check;
mod frontend;
mod issue;
mod php;
mod registry;
mod symfony_routing;

#[cfg(test)]
mod tests;

use async_trait::async_trait;
use serde_json::{json, Map, Value};
use std::path::{Component, Path, PathBuf};

use crate::llm::{LlmResult, Tool};

use check::{Check, CheckContext, Shell};
use issue::Issue;

pub struct LintCodeTool {
    parameters: Map<String, Value>,
}

impl LintCodeTool {
    pub fn new() -> Self {
        let checks = registry::default_checks();
        let catalogue = registry::describe_checks(&checks);
        let check_ids: Vec<&str> = checks.iter().map(|check| check.id()).collect();

        Self {
            parameters: json!({
                "type": "object",
                "properties": {
                    "shell": {
                        "type": "string",
                        "enum": ["frontend", "backend", "all"],
                        "description": "Target shell to lint: 'frontend' (JS/TS + menu.json), 'backend' (PHP + Symfony routing), or 'all'."
                    },
                    "path": {
                        "type": "string",
                        "description": "Optional relative file or folder path within the shell's src directory to lint. If omitted, lints all src files."
                    },
                    "deep": {
                        "type": "boolean",
                        "description": "Run slower command-backed checks (Symfony routing audit, Doctrine schema, analytics harness). Defaults to true for 'backend'/'all' and false for 'frontend'."
                    },
                    "checks": {
                        "type": "array",
                        "items": { "type": "string", "enum": check_ids },
                        "description": format!("Run only these checks. Available: {catalogue}")
                    },
                    "skipChecks": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Check ids to skip. Same ids as 'checks'."
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
        "Lints an AdminPanel project for syntax errors and structural problems: PHP syntax, ESLint, `menu-items/menu.json`, Doctrine mappings, the analytics harness, and Symfony route registration. Route checks catch controllers that compile but never register (e.g. a removed `Routing\\Annotation\\Route` import), which otherwise show up only as 404s."
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

        let relative_path = validated_relative_path(&args)?;

        // Command-backed checks are opt-out: they are the ones that catch
        // structural problems, so default them on for backend/all.
        let deep = match args.get("deep").and_then(Value::as_bool) {
            Some(value) => value,
            None => shell != "frontend",
        };

        let checks = registry::default_checks();
        let selection = Selection::from_args(&args, &checks)?;

        let project_path = args
            .get("projectPath")
            .and_then(Value::as_str)
            .filter(|path| !path.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("content").join("dummy-project"));

        let ctx = CheckContext {
            project_path: &project_path,
            relative_path,
            deep,
        };

        let requested_shells: &[Shell] = match shell {
            "backend" => &[Shell::Backend],
            "frontend" => &[Shell::Frontend],
            _ => &[Shell::Backend, Shell::Frontend],
        };

        let mut issues: Vec<Issue> = Vec::new();
        let mut skipped_for_deep: Vec<&str> = Vec::new();

        for check in checks.iter() {
            if !selection.includes(check.id()) {
                continue;
            }
            if !requested_shells
                .iter()
                .any(|shell| check.shells().includes(*shell))
            {
                continue;
            }
            if check.requires_deep() && !ctx.deep {
                skipped_for_deep.push(check.id());
                continue;
            }

            match check.run(&ctx) {
                Ok(found) => issues.extend(found),
                // A failing check is reported, never fatal: one broken command
                // must not hide the results of the checks that did work.
                Err(error) => issues.push(Issue::error(
                    check.shells().primary(),
                    check.id(),
                    "lint",
                    1,
                    format!("Check '{}' could not run: {error}", check.id()),
                )),
            }
        }

        let total_errors = issues.iter().filter(|issue| issue.is_error()).count();
        let total_warnings = issues.len() - total_errors;

        let success = total_errors == 0;
        let mut message = if success {
            if total_warnings > 0 {
                format!("Lint completed successfully with {total_warnings} warning(s) and 0 errors.")
            } else {
                "Lint passed successfully with 0 errors and 0 warnings.".to_string()
            }
        } else {
            format!("Lint found {total_errors} error(s) and {total_warnings} warning(s).")
        };

        if !skipped_for_deep.is_empty() {
            message.push_str(&format!(
                " Skipped (requires `deep: true`): {}.",
                skipped_for_deep.join(", ")
            ));
        }

        Ok(json!({
            "success": success,
            "message": message,
            "totalErrors": total_errors,
            "totalWarnings": total_warnings,
            "issues": issues.iter().map(Issue::to_json).collect::<Vec<_>>()
        }))
    }
}

/// Which checks the caller asked for.
struct Selection<'a> {
    only: Option<Vec<&'a str>>,
    skip: Vec<&'a str>,
}

impl<'a> Selection<'a> {
    fn from_args(args: &'a Value, checks: &[Box<dyn Check>]) -> LlmResult<Self> {
        let known: Vec<&str> = checks.iter().map(|check| check.id()).collect();
        let only = read_id_list(args, "checks", &known)?;
        let skip = read_id_list(args, "skipChecks", &known)?.unwrap_or_default();
        Ok(Self { only, skip })
    }

    fn includes(&self, id: &str) -> bool {
        if self.skip.iter().any(|skipped| *skipped == id) {
            return false;
        }
        match &self.only {
            Some(only) => only.iter().any(|selected| *selected == id),
            None => true,
        }
    }
}

fn read_id_list<'a>(
    args: &'a Value,
    key: &str,
    known: &[&str],
) -> LlmResult<Option<Vec<&'a str>>> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }

    let list = value
        .as_array()
        .ok_or_else(|| format!("`{key}` must be an array of check ids"))?;

    let mut ids = Vec::with_capacity(list.len());
    for entry in list {
        let id = entry
            .as_str()
            .ok_or_else(|| format!("`{key}` must contain strings only"))?;
        if !known.contains(&id) {
            return Err(format!(
                "Unknown check id '{id}'. Available: {}",
                known.join(", ")
            )
            .into());
        }
        ids.push(id);
    }

    Ok(Some(ids))
}

fn validated_relative_path(args: &Value) -> LlmResult<Option<&str>> {
    let Some(path) = args
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
    else {
        return Ok(None);
    };

    let parsed = Path::new(path);
    let escapes = parsed.is_absolute()
        || parsed.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        });

    if escapes {
        return Err("path must be a safe relative path".into());
    }

    Ok(Some(path))
}
