use super::Skill;
use super::skill_markdown;

/// Name of the markdown file that holds this skill's instructions.
///
/// The file lives under `src/skills/` and is loaded by [`skill_markdown::load`].
const CONTENT_FILE: &str = "profile_management.md";

/// Skill for managing user profile and extending profile fields via sync_schema.
pub struct ProfileManagementSkill {
    tags: Vec<String>,
}

impl ProfileManagementSkill {
    pub fn new() -> Self {
        Self {
            tags: vec!["profile", "user", "avatar", "sync_schema", "backend", "frontend"]
                .into_iter()
                .map(str::to_string)
                .collect(),
        }
    }
}

impl Default for ProfileManagementSkill {
    fn default() -> Self {
        Self::new()
    }
}

impl Skill for ProfileManagementSkill {
    fn name(&self) -> &str {
        "profile_management"
    }

    fn description(&self) -> &str {
        "Teaches the agent how to read user profile information and add additional fields to the profile resource using sync_schema."
    }

    fn content(&self) -> &str {
        skill_markdown::load(CONTENT_FILE)
    }

    fn category(&self) -> Option<&str> {
        Some("profile")
    }

    fn tags(&self) -> &[String] {
        &self.tags
    }
}
