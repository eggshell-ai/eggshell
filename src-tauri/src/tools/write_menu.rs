use async_trait::async_trait;
use serde_json::{json, Map, Value};
use std::path::PathBuf;

use crate::llm::{LlmResult, Tool};

pub struct WriteMenuTool {
    parameters: Map<String, Value>,
}

impl WriteMenuTool {
    pub fn new() -> Self {
        Self {
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "The menu path in dot notation (e.g., Inventory.Products)."
                    },
                    "route": {
                        "type": "string",
                        "description": "The route path for the menu item (e.g., /products)."
                    },
                    "icon": {
                        "type": "string",
                        "description": "The icon name from @ant-design/icons (e.g., DashboardOutlined)."
                    },
                    "after": {
                        "type": "string",
                        "description": "Optional menu item name to insert after."
                    },
                    "permission": {
                        "type": "string",
                        "description": "Optional permission string (e.g., products.view)."
                    },
                    "projectPath": {
                        "type": "string",
                        "description": "Overrides the target project directory."
                    }
                },
                "required": ["name", "route", "icon"]
            })
            .as_object()
            .expect("write_menu parameters must be an object")
            .clone(),
        }
    }
}

impl Default for WriteMenuTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WriteMenuTool {
    fn name(&self) -> &str {
        "write_menu"
    }

    fn description(&self) -> &str {
        "Adds or updates a menu item in the menu.json file."
    }

    fn parameters(&self) -> &Map<String, Value> {
        &self.parameters
    }

    async fn execute(&self, args: Value) -> LlmResult<Value> {
        let name = required_string(&args, "name")?;
        let route = required_string(&args, "route")?;
        let icon = required_string(&args, "icon")?;
        let after = optional_string(&args, "after");
        let permission = optional_string(&args, "permission");
        let project_path = args
            .get("projectPath")
            .and_then(Value::as_str)
            .filter(|path| !path.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("content").join("dummy-project"));

        let menu_path = project_path.join("frontend").join("schemas").join("menu.json");
        if let Some(parent) = menu_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut menu = if menu_path.exists() {
            let content = std::fs::read_to_string(&menu_path)?;
            serde_json::from_str::<Value>(&content)?
        } else {
            Value::Array(Vec::new())
        };
        let items = menu
            .as_array_mut()
            .ok_or_else(|| "menu.json must contain a JSON array".to_string())?;

        let mut menu_item = json!({ "name": name, "route": route, "icon": icon });
        if let Some(permission) = permission {
            menu_item["permission"] = Value::String(permission);
        }

        if let Some(index) = items.iter().position(|item| item.get("name").and_then(Value::as_str) == Some(name.as_str())) {
            items[index] = menu_item;
        } else {
            let insert_index = after
                .as_deref()
                .and_then(|value| items.iter().position(|item| item.get("name").and_then(Value::as_str) == Some(value)))
                .map(|index| index + 1)
                .unwrap_or_else(|| items.len().saturating_sub(1));
            items.insert(insert_index, menu_item);
        }

        std::fs::write(&menu_path, serde_json::to_string_pretty(&menu)?)?;
        Ok(json!({
            "success": true,
            "message": "Menu item added successfully.",
            "details": {
                "name": name,
                "route": route,
                "icon": icon,
                "menuPath": menu_path,
                "timestamp": timestamp()
            }
        }))
    }
}

fn required_string(args: &Value, name: &str) -> LlmResult<String> {
    args.get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("{name} is required and must be a non-empty string").into())
}

fn optional_string(args: &Value, name: &str) -> Option<String> {
    args.get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}
