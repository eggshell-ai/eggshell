use async_trait::async_trait;
use serde_json::{json, Map, Value};
use std::path::{Component, PathBuf};

use crate::llm::{LlmResult, Tool};

pub struct WritePageTool {
    parameters: Map<String, Value>,
}

impl WritePageTool {
    pub fn new() -> Self {
        Self {
            parameters: json!({
                "type": "object",
                "properties": {
                    "route": { "type": "string", "description": "The route path for the page (e.g., /products)." },
                    "code": { "type": "string", "description": "The React component code for the page." },
                    "projectPath": { "type": "string", "description": "Overrides the target project directory." }
                },
                "required": ["route", "code"]
            })
            .as_object()
            .expect("write_page parameters must be an object")
            .clone(),
        }
    }
}

impl Default for WritePageTool {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Tool for WritePageTool {
    fn name(&self) -> &str { "write_page" }

    fn description(&self) -> &str {
        "Creates a new page component with the given code and optionally adds it to the menu."
    }

    fn parameters(&self) -> &Map<String, Value> { &self.parameters }

    async fn execute(&self, args: Value) -> LlmResult<Value> {
        let route = args.get("route").and_then(Value::as_str)
            .ok_or_else(|| "route is required".to_string())?;
        let code = args.get("code").and_then(Value::as_str)
            .ok_or_else(|| "code is required and must be a string".to_string())?;
        let project_path = args.get("projectPath").and_then(Value::as_str)
            .filter(|path| !path.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("content").join("dummy-project"));

        let segments = route.trim().trim_start_matches('/').split('/')
            .filter(|segment| !segment.is_empty()).collect::<Vec<_>>();
        if segments.iter().any(|segment| {
            PathBuf::from(segment).components().any(|component| {
                matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
            })
        }) {
            return Err("route must contain only relative path segments".into());
        }

        let page_path = project_path.join("frontend").join("src").join("app")
            .join("(dashboard)").join(segments.iter().collect::<PathBuf>()).join("page.tsx");
        if let Some(parent) = page_path.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::write(&page_path, code)?;

        Ok(json!({
            "success": true,
            "message": "Page created successfully.",
            "details": { "route": route, "pagePath": page_path, "timestamp": timestamp() }
        }))
    }
}

fn timestamp() -> String {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string()).unwrap_or_else(|_| "0".to_string())
}
