use async_trait::async_trait;
use serde_json::{json, Map, Value};

use crate::llm::{LlmResult, Skill, Tool};

pub struct LoadSkillTool {
    parameters: Map<String, Value>,
    skills: Vec<Box<dyn Skill>>,
}

impl LoadSkillTool {
    pub fn new(skills: Vec<Box<dyn Skill>>) -> Self {
        Self {
            parameters: json!({
                "type": "object",
                "properties": {
                    "skillName": {
                        "type": "string",
                        "description": "The name of the skill to load (e.g., crud_creation)"
                    },
                    "path": {
                        "type": "string",
                        "description": "Optional path of a file bundled with the skill, relative to the skill folder (e.g., docs/row_actions.md). When omitted, the skill's SKILL.md instructions are returned."
                    }
                },
                "required": ["skillName"]
            })
            .as_object()
            .expect("load_skill parameters must be an object")
            .clone(),
            skills,
        }
    }
}

#[async_trait]
impl Tool for LoadSkillTool {
    fn name(&self) -> &str {
        "load_skill"
    }

    fn description(&self) -> &str {
        "Loads a skill's instructions by name. Skills may reference supporting documents inside their own folder; pass the skill name and the file path to read one."
    }

    fn parameters(&self) -> &Map<String, Value> {
        &self.parameters
    }

    async fn execute(&self, args: Value) -> LlmResult<Value> {
        let skill_name = args
            .get("skillName")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| "skillName is required and must be a non-empty string".to_string())?;

        let Some(skill) = self.skills.iter().find(|skill| skill.name() == skill_name) else {
            return Ok(json!({
                "success": false,
                "message": format!("Skill '{skill_name}' not found."),
                "error": "Skill not found"
            }));
        };

        // When a path is supplied, return only that bundled file so the agent
        // can pull in supporting material referenced by the skill itself.
        let path = args
            .get("path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|path| !path.is_empty());

        if let Some(path) = path {
            let Some(content) = skill.read(path) else {
                return Ok(json!({
                    "success": false,
                    "message": format!("File '{path}' not found in skill '{skill_name}'."),
                    "error": "File not found",
                    "details": {
                        "skillName": skill.name(),
                        "availableFiles": skill.files()
                    }
                }));
            };

            return Ok(json!({
                "success": true,
                "message": format!("File '{path}' of skill '{skill_name}' loaded successfully."),
                "details": {
                    "skillName": skill.name(),
                    "path": path,
                    "content": content
                }
            }));
        }

        Ok(json!({
            "success": true,
            "message": format!("Skill '{skill_name}' loaded successfully."),
            "details": {
                "skillName": skill.name(),
                "description": skill.description(),
                "category": skill.category(),
                "tags": skill.tags(),
                "files": skill.files(),
                "content": skill.content()
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool() -> LoadSkillTool {
        LoadSkillTool::new(crate::llm::default_skills())
    }

    /// Runs a future to completion; the tool implementation never actually
    /// yields, so a single-threaded runtime is enough.
    fn run<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime builds")
            .block_on(future)
    }

    #[test]
    fn loads_a_skill_by_name() {
        let result = run(tool().execute(json!({
            "skillName": "crud_creation"
        })))
        .expect("execute succeeds");

        assert_eq!(result["success"], json!(true));
        assert_eq!(result["details"]["skillName"], json!("crud_creation"));
        assert!(result["details"]["content"]
            .as_str()
            .unwrap()
            .contains("CRUD Creation Skill"));
        // Documents are no longer advertised in the skill response.
        assert!(result["details"].get("documents").is_none());
    }

    #[test]
    fn loads_a_referenced_document_by_path() {
        let result = run(tool().execute(json!({
            "skillName": "crud_creation",
            "path": "docs/row_actions.md"
        })))
        .expect("execute succeeds");

        assert_eq!(result["success"], json!(true));
        assert_eq!(result["details"]["path"], json!("docs/row_actions.md"));
        assert!(result["details"]["content"]
            .as_str()
            .unwrap()
            .contains("Custom Row Actions"));
    }

    #[test]
    fn unknown_path_lists_available_files() {
        let result = run(tool().execute(json!({
            "skillName": "crud_creation",
            "path": "docs/missing.md"
        })))
        .expect("execute succeeds");

        assert_eq!(result["success"], json!(false));
        assert_eq!(result["error"], json!("File not found"));
        let files = result["details"]["availableFiles"].as_array().unwrap();
        assert!(files.iter().any(|file| file == "docs/row_actions.md"));
    }

    #[test]
    fn unknown_skill_is_reported() {
        let result = run(tool().execute(json!({
            "skillName": "does_not_exist"
        })))
        .expect("execute succeeds");

        assert_eq!(result["success"], json!(false));
        assert_eq!(result["error"], json!("Skill not found"));
    }
}
