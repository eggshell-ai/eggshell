use async_trait::async_trait;
use serde_json::{json, Map, Value};
use std::path::PathBuf;

use crate::llm::{LlmResult, Tool};
use crate::modules;

/// Installs an optional feature module into the current project.
///
/// The module id enum is captured when the tool is constructed, the same way
/// `LintCodeTool` captures its check ids: `parameters()` returns a borrow, so the
/// allowed values have to exist in the struct.
pub struct EnableModuleTool {
    parameters: Map<String, Value>,
}

impl EnableModuleTool {
    pub fn new() -> Self {
        let ids = modules::available_ids();
        let catalogue = modules::catalogue()
            .into_iter()
            .map(|module| format!("{} ({})", module.id, module.name))
            .collect::<Vec<_>>()
            .join("; ");

        Self {
            parameters: json!({
                "type": "object",
                "properties": {
                    "module": {
                        "type": "string",
                        "enum": ids,
                        "description": format!("The id of the module to install. Available: {catalogue}")
                    },
                    "projectPath": {
                        "type": "string",
                        "description": "Overrides the target project directory."
                    }
                },
                "required": ["module"]
            })
            .as_object()
            .expect("enable_module parameters must be an object")
            .clone(),
        }
    }
}

impl Default for EnableModuleTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for EnableModuleTool {
    fn name(&self) -> &str {
        "enable_module"
    }

    fn description(&self) -> &str {
        "Installs an optional feature module into the project: copies its backend and frontend files, adds its Composer/npm packages, registers its Symfony bundles, and returns the module's instructions. Safe to call again; an installed module is left untouched."
    }

    fn parameters(&self) -> &Map<String, Value> {
        &self.parameters
    }

    async fn execute(&self, args: Value) -> LlmResult<Value> {
        let module = args
            .get("module")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| "module is required and must be a non-empty string".to_string())?;

        // An unknown module is a correctable mistake, not a failure: report it
        // with the alternatives so the model can pick one.
        if modules::find(module).is_none() {
            return Ok(json!({
                "success": false,
                "message": format!("Module '{module}' does not exist."),
                "error": "Unknown module",
                "details": { "availableModules": modules::available_ids() }
            }));
        }

        let project = args
            .get("projectPath")
            .and_then(Value::as_str)
            .filter(|path| !path.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("content").join("dummy-project"));

        match modules::install(&project, module).await {
            Ok(report) => Ok(modules::report_to_value(&report)),
            Err(error) => Ok(json!({
                "success": false,
                "message": format!("Module '{module}' could not be installed: {error}"),
                "error": error.to_string(),
                "details": { "module": module }
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime builds")
            .block_on(future)
    }

    #[test]
    fn exposes_available_modules_in_parameters() {
        let tool = EnableModuleTool::new();
        let ids = tool.parameters()["properties"]["module"]["enum"]
            .as_array()
            .expect("enum present");
        assert!(ids.iter().any(|id| id == "pdf-export"));
    }

    #[test]
    fn unknown_module_is_a_soft_failure() {
        let result = run(EnableModuleTool::new().execute(json!({ "module": "nope" })))
            .expect("execute succeeds");
        assert_eq!(result["success"], json!(false));
        assert_eq!(result["error"], json!("Unknown module"));
        assert!(result["details"]["availableModules"].is_array());
    }
}
