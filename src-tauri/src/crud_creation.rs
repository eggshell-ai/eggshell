use super::Skill;
use super::skill_markdown;
use super::SkillDocument;

/// Name of the markdown file that holds this skill's instructions.
///
/// The file lives under `src/skills/` and is loaded by [`skill_markdown::load`].
const CONTENT_FILE: &str = "crud_creation.md";

/// Skill for creating a complete CRUD interface across the application stack.
pub struct CrudCreationSkill {
    tags: Vec<String>,
    /// On-demand documents bundled with this skill.
    ///
    /// These keep the main instructions concise: deeper reference material is
    /// only pulled in when the agent explicitly loads the document.
    documents: Vec<SkillDocument>,
}

impl CrudCreationSkill {
    pub fn new() -> Self {
        Self {
            tags: vec!["crud", "database", "frontend", "backend", "symfony", "react"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            documents: vec![
                SkillDocument {
                    name: "row_actions",
                    description: "Detailed reference for custom row actions: backend path actions, custom JavaScript handlers with an injected helper API, and reusable form/popup actions that submit to custom endpoints.",
                    content: skill_markdown::load("row_actions.md"),
                },
                SkillDocument {
                    name: "custom_ui",
                    description: "How to add custom JavaScript behavior, bespoke functionality, and reusable popups/forms to resources that go beyond standard CRUD.",
                    content: skill_markdown::load("custom_ui.md"),
                },
            ],
        }
    }
}

impl Default for CrudCreationSkill {
    fn default() -> Self { Self::new() }
}

impl Skill for CrudCreationSkill {
    fn name(&self) -> &str { "crud_creation" }
    fn description(&self) -> &str {
        "Teaches the agent how to create a complete CRUD interface with database schema, backend controller, frontend pages, and menu integration."
    }
    fn content(&self) -> &str { skill_markdown::load(CONTENT_FILE) }
    fn category(&self) -> Option<&str> { Some("development") }
    fn tags(&self) -> &[String] { &self.tags }
    fn documents(&self) -> &[SkillDocument] { &self.documents }
}
