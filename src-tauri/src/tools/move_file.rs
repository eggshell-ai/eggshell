use async_trait::async_trait;
use serde_json::{json, Map, Value};
use std::path::{Component, Path, PathBuf};

use crate::llm::{LlmResult, Tool};

pub struct MoveFileTool {
    parameters: Map<String, Value>,
}

impl MoveFileTool {
    pub fn new() -> Self {
        Self {
            parameters: json!({
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "Absolute path of the uploaded file to move."
                    },
                    "shell": {
                        "type": "string",
                        "enum": ["frontend", "backend"],
                        "description": "Target shell directory (frontend or backend)."
                    },
                    "path": {
                        "type": "string",
                        "description": "Relative destination path inside the shell src directory."
                    },
                    "projectPath": {
                        "type": "string",
                        "description": "Overrides the target project directory."
                    }
                },
                "required": ["source", "shell", "path"]
            })
            .as_object()
            .expect("move_file parameters must be an object")
            .clone(),
        }
    }
}

impl Default for MoveFileTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for MoveFileTool {
    fn name(&self) -> &str {
        "move_file"
    }

    fn description(&self) -> &str {
        "Moves an uploaded file into the frontend or backend shell within the src directory."
    }

    fn parameters(&self) -> &Map<String, Value> {
        &self.parameters
    }

    async fn execute(&self, args: Value) -> LlmResult<Value> {
        let source = args
            .get("source")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .ok_or_else(|| "source is required and must be a non-empty path".to_string())?;
        let source = PathBuf::from(source);
        if !source.is_file() {
            return Err(format!("source file not found: {}", source.display()).into());
        }

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
                matches!(component, Component::RootDir | Component::Prefix(_))
            })
        {
            return Err("path must be a relative path".into());
        }

        let project_path = args
            .get("projectPath")
            .and_then(Value::as_str)
            .filter(|path| !path.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("content").join("dummy-project"));
        // Logos land in `public/`, one level above `src`, so parent components
        // are allowed — but only while the resolved target stays inside the
        // project directory, which keeps the tool from writing elsewhere.
        let target = project_path.join(shell).join("src").join(relative);
        if !target.starts_with(&project_path) {
            return Err("path must remain within the project directory".into());
        }

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Uploaded logos and assets are copied rather than removed from the
        // upload directory: the user may attach the same file again later.
        std::fs::copy(&source, &target)?;

        Ok(json!({
            "success": true,
            "message": "File moved successfully.",
            "details": {
                "source": source,
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
