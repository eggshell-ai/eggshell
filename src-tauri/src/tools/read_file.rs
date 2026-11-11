use async_trait::async_trait;
use serde_json::{json, Map, Value};
use std::fs;

use crate::llm::{LlmResult, Tool};

use super::list_directory::{render_tree, MAX_ENTRIES};
use super::paths::resolve_shell_path;

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
        "Reads the content of a file from either the frontend or backend shell within src directory. Use list_directory first to discover exact paths: if this is called on a directory it returns that directory's contents instead of an error, along with a warning."
    }

    fn parameters(&self) -> &Map<String, Value> {
        &self.parameters
    }

    async fn execute(&self, args: Value) -> LlmResult<Value> {
        let resolved = resolve_shell_path(&args, false)?;
        let target = resolved.target;

        if !target.exists() {
            return Ok(json!({
                "success": false,
                "message": format!("File not found at path: {}", target.display())
            }));
        }

        // Reading a directory is a common modelling mistake. Rather than fail,
        // answer with the listing the model actually wanted and say which tool
        // should have been used, so the next call is correct.
        if target.is_dir() {
            let listing = render_tree(&target, 1, false, MAX_ENTRIES);
            let entry_count = fs::read_dir(&target)
                .map(|entries| entries.flatten().count())
                .unwrap_or(0);
            return Ok(json!({
                "success": false,
                "warning": "read_file was called on a directory. Use list_directory to explore directories.",
                "message": format!(
                    "`{}` is a directory, not a file. Its immediate contents are listed below; call list_directory for a deeper listing.",
                    resolved.relative
                ),
                "details": {
                    "shell": resolved.shell,
                    "path": resolved.relative,
                    "filePath": target,
                    "isDirectory": true,
                    "entryCount": entry_count,
                    "tree": listing.to_text(),
                    "truncated": listing.truncated,
                    "shown": listing.shown,
                    "hidden": listing.hidden,
                    "hint": format!(
                        "list_directory({{ shell: \"{}\", path: \"{}\" }}) lists this directory with depth 2 and collapses vendor/node_modules/build output.",
                        resolved.shell,
                        resolved.relative
                    ),
                }
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
                "shell": resolved.shell,
                "path": resolved.relative,
                "filePath": target,
                "content": content
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixture(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("eggshell-read-{name}-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create fixture root");
        root
    }

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(path, contents).expect("write fixture file");
    }

    #[tokio::test]
    async fn reading_a_file_returns_its_content() {
        let root = fixture("file");
        write(&root.join("backend/src/Entity/Product.php"), "<?php");

        let result = ReadFileTool::new()
            .execute(json!({
                "shell": "backend",
                "path": "Entity/Product.php",
                "projectPath": root.to_string_lossy()
            }))
            .await
            .expect("tool executes");

        assert_eq!(result["success"], true);
        assert_eq!(result["details"]["content"], "<?php");
        assert_eq!(result["details"]["path"], "Entity/Product.php");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn reading_a_directory_returns_a_listing_and_a_warning() {
        let root = fixture("directory");
        write(&root.join("frontend/src/app/(dashboard)/products/page.tsx"), "page");
        write(&root.join("frontend/src/app/(dashboard)/orders/page.tsx"), "page");

        let result = ReadFileTool::new()
            .execute(json!({
                "shell": "frontend",
                "path": "app/(dashboard)",
                "projectPath": root.to_string_lossy()
            }))
            .await
            .expect("tool executes");

        assert_eq!(result["success"], false);
        assert!(result["warning"].as_str().unwrap().contains("list_directory"));
        assert_eq!(result["details"]["isDirectory"], true);
        // Depth 1: the immediate children only, which is the hint the model needs.
        assert_eq!(result["details"]["tree"], "orders/\nproducts/");
        assert!(result["details"]["hint"].as_str().unwrap().contains("list_directory"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn traversal_is_still_rejected() {
        let error = ReadFileTool::new()
            .execute(json!({ "shell": "frontend", "path": "../../etc/passwd" }))
            .await
            .expect_err("traversal rejected");
        assert!(error.to_string().contains("must remain within"));
    }
}
