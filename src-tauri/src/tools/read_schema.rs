use async_trait::async_trait;
use serde_json::{json, Map, Value};
use std::path::PathBuf;

use super::sync_schema::schema_file_path;
use crate::llm::{LlmResult, Tool};

pub struct ReadSchemaTool {
    parameters: Map<String, Value>,
}

impl ReadSchemaTool {
    pub fn new() -> Self {
        Self {
            parameters: json!({
                "type": "object",
                "properties": {
                    "projectPath": {
                        "type": "string",
                        "description": "Overrides the target project directory."
                    },
                    "resource": {
                        "type": "string",
                        "description": "Optional name of a specific resource to read schema for. If omitted, all schemas are returned."
                    }
                }
            })
            .as_object()
            .expect("read_schema parameters must be an object")
            .clone(),
        }
    }
}

impl Default for ReadSchemaTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ReadSchemaTool {
    fn name(&self) -> &str {
        "read_schema"
    }

    fn description(&self) -> &str {
        "Reads already written resource schemas for the project from the schema JSON file."
    }

    fn parameters(&self) -> &Map<String, Value> {
        &self.parameters
    }

    async fn execute(&self, args: Value) -> LlmResult<Value> {
        let project_path = args
            .get("projectPath")
            .and_then(Value::as_str)
            .filter(|path| !path.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("content").join("dummy-project"));

        let schema_path = schema_file_path(&project_path);

        if !schema_path.exists() {
            return Ok(json!({
                "success": false,
                "message": format!("No schema file found at: {}", schema_path.display()),
                "details": {
                    "filePath": schema_path,
                    "resources": []
                }
            }));
        }

        let content = std::fs::read_to_string(&schema_path)?;
        let schema_val: Value = match serde_json::from_str(&content) {
            Ok(val) => val,
            Err(e) => {
                return Ok(json!({
                    "success": false,
                    "message": format!("Failed to parse schema JSON file at {}: {}", schema_path.display(), e),
                    "details": {
                        "filePath": schema_path,
                        "rawContent": content
                    }
                }));
            }
        };

        let all_resources: Vec<Value> = match &schema_val {
            Value::Array(arr) => arr.clone(),
            Value::Object(map) => match map.get("resources") {
                Some(Value::Array(arr)) => arr.clone(),
                _ => vec![],
            },
            _ => vec![],
        };

        let resource_filter = args
            .get("resource")
            .or_else(|| args.get("name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());

        if let Some(target_name) = resource_filter {
            let found = all_resources.iter().find(|r| {
                r.get("name")
                    .and_then(Value::as_str)
                    .map(|n| n.eq_ignore_ascii_case(target_name))
                    .unwrap_or(false)
            });

            match found {
                Some(res) => Ok(json!({
                    "success": true,
                    "message": format!("Schema for resource '{}' read successfully.", target_name),
                    "resource": res,
                    "details": {
                        "filePath": schema_path,
                        "resource": res
                    }
                })),
                None => {
                    let available: Vec<&str> = all_resources
                        .iter()
                        .filter_map(|r| r.get("name").and_then(Value::as_str))
                        .collect();
                    Ok(json!({
                        "success": false,
                        "message": format!("Resource '{}' not found in schema file.", target_name),
                        "details": {
                            "filePath": schema_path,
                            "availableResources": available
                        }
                    }))
                }
            }
        } else {
            Ok(json!({
                "success": true,
                "message": "Schemas read successfully.",
                "resources": all_resources,
                "schema": schema_val,
                "details": {
                    "filePath": schema_path,
                    "resources": all_resources
                }
            }))
        }
    }
}
