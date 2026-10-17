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
                    "document": {
                        "type": "string",
                        "description": "Optional name of a specific document within the skill to load (e.g., row_actions). When omitted, the skill's main instructions are returned."
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
        "Loads a skill by name to teach the agent how to perform specific tasks. Optionally loads a specific document within that skill when more detail is needed."
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

        // When a document name is supplied, return only that document's body so
        // the agent can pull in deeper reference material on demand.
        let document = args
            .get("document")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty());

        if let Some(document_name) = document {
            let Some(document) = skill
                .documents()
                .iter()
                .find(|document| document.name == document_name)
            else {
                let available: Vec<&str> =
                    skill.documents().iter().map(|doc| doc.name).collect();
                return Ok(json!({
                    "success": false,
                    "message": format!(
                        "Document '{document_name}' not found in skill '{skill_name}'."
                    ),
                    "error": "Document not found",
                    "details": {
                        "skillName": skill.name(),
                        "availableDocuments": available
                    }
                }));
            };

            return Ok(json!({
                "success": true,
                "message": format!(
                    "Document '{document_name}' of skill '{skill_name}' loaded successfully."
                ),
                "details": {
                    "skillName": skill.name(),
                    "documentName": document.name,
                    "description": document.description,
                    "content": document.content
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
                "documents": skill.documents().iter().map(|doc| json!({
                    "name": doc.name,
                    "description": doc.description
                })).collect::<Vec<_>>(),
                "content": skill.content()
            }
        }))
    }
}
