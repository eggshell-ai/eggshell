use super::Skill;
use super::skill_markdown;

/// Name of the markdown file that holds this skill's instructions.
///
/// The file lives under `src/skills/` and is loaded by [`skill_markdown::load`].
const CONTENT_FILE: &str = "notification_creation.md";

/// Skill for sending notifications to users, roles, or all users in the system.
pub struct NotificationCreationSkill {
    tags: Vec<String>,
}

impl NotificationCreationSkill {
    pub fn new() -> Self {
        Self {
            tags: vec!["notification", "backend", "symfony", "service", "alert"]
                .into_iter()
                .map(str::to_string)
                .collect(),
        }
    }
}

impl Default for NotificationCreationSkill {
    fn default() -> Self {
        Self::new()
    }
}

impl Skill for NotificationCreationSkill {
    fn name(&self) -> &str {
        "notification_creation"
    }

    fn description(&self) -> &str {
        "Teaches the agent how to send in-app notifications to users, roles, or broadcast to all users via NotificationService in Symfony."
    }

    fn content(&self) -> &str {
        skill_markdown::load(CONTENT_FILE)
    }

    fn category(&self) -> Option<&str> {
        Some("notifications")
    }

    fn tags(&self) -> &[String] {
        &self.tags
    }
}
