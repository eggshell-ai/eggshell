use async_trait::async_trait;
use serde_json::{json, Map, Value};
use std::path::{Component, Path, PathBuf};

use crate::llm::{LlmResult, Tool};

pub struct ReadFileTool {
    parameters: Map<String, Value>,
}

impl ReadFileTool {
    pub fn new() -> Self {
        Self {
            parameters: json!({
                "type": "object",
                "properties": {
                    "shell": {
                        "type": "string",
                        "enum": ["frontend", "backend"],
                        "description": "Target shell directory (frontend or backend)."
                    },
                    "path": {
                        "type": "string",
                        "description": "Relative file path inside the src directory."
                    },
                    "projectPath": {
                        "type": "string",
                        "description": "Overrides the target project directory."
                    }
                },
                "required": ["shell", "path"]
            })
            .as_object()
            .expect("read_file parameters must be an object")
            .clone(),
        }
    }
}

impl Default for ReadFileTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Reads the content of a file from either the frontend or backend shell within src directory."
    }

    fn parameters(&self) -> &Map<String, Value> {
        &self.parameters
    }

    async fn execute(&self, args: Value) -> LlmResult<Value> {
        let shell = args
            .get("shell")
            .and_then(Value::as_str)
            .ok_or_else(|| "shell is required".to_string())?;
        if !matches!(shell, "frontend" | "backend") {
            return Err("shell must be either 'frontend' or 'backend'".into());
        }

        let relative_path = args
            .get("path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .ok_or_else(|| "path is required and must be a non-empty relative path".to_string())?;

        let relative = Path::new(relative_path);
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
        let target = project_path.join(shell).join("src").join(relative);

        if !target.exists() {
            return Ok(json!({
                "success": false,
                "message": format!("File not found at path: {}", target.display())
            }));
        }
        if !target.is_file() {
            return Err(format!("Path is not a file: {}", target.display()).into());
        }

        let content = std::fs::read_to_string(&target)?;
        Ok(json!({
            "success": true,
            "message": "File read successfully.",
            "details": {
                "shell": shell,
                "path": relative_path,
                "filePath": target,
                "content": content
            }
        }))
    }
}
