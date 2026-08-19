use async_trait::async_trait;
use serde_json::{json, Map, Value};
use std::path::{Component, Path, PathBuf};

use crate::llm::{LlmResult, Tool};

pub struct WriteFileTool {
    parameters: Map<String, Value>,
}

impl WriteFileTool {
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
                    "content": {
                        "type": "string",
                        "description": "The file content to write."
                    },
                    "projectPath": {
                        "type": "string",
                        "description": "Overrides the target project directory."
                    }
                },
                "required": ["shell", "path", "content"]
            })
            .as_object()
            .expect("write_file parameters must be an object")
            .clone(),
        }
    }
}

impl Default for WriteFileTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Writes content to a file in either the frontend or backend shell within src directory."
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

        let content = args
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| "content is required and must be a string".to_string())?;
        let project_path = args
            .get("projectPath")
            .and_then(Value::as_str)
            .filter(|path| !path.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("content").join("dummy-project"));
        let target = project_path.join(shell).join("src").join(relative);

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, content)?;

        Ok(json!({
            "success": true,
            "message": "File written successfully.",
            "details": {
                "shell": shell,
                "path": relative_path,
                "filePath": target,
                "timestamp": format_timestamp()
            }
        }))
    }
}

fn format_timestamp() -> String {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => duration.as_secs().to_string(),
        Err(_) => "0".to_string(),
    }
}
