use super::Skill;
use super::skill_markdown;

/// Name of the markdown file that holds this skill's instructions.
///
/// The file lives under `src/skills/` and is loaded by [`skill_markdown::load`].
const CONTENT_FILE: &str = "qa_testing.md";

/// Skill for visual and functional QA testing using integrated browser tools.
pub struct QaTestingSkill {
    tags: Vec<String>,
}

impl QaTestingSkill {
    pub fn new() -> Self {
        Self {
            tags: vec!["qa", "testing", "browser", "verification", "visual"]
                .into_iter()
                .map(str::to_string)
                .collect(),
        }
    }
}

impl Default for QaTestingSkill {
    fn default() -> Self {
        Self::new()
    }
}

impl Skill for QaTestingSkill {
    fn name(&self) -> &str {
        "qa_testing"
    }

    fn description(&self) -> &str {
        "Teaches the agent how to test generated web applications using integrated browser tools, navigate routes, verify forms and CRUD features, and detect issues."
    }

    fn content(&self) -> &str {
        skill_markdown::load(CONTENT_FILE)
    }

    fn category(&self) -> Option<&str> {
        Some("quality_assurance")
    }

    fn tags(&self) -> &[String] {
        &self.tags
    }
}
