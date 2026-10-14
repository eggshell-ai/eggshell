use super::Skill;
use super::skill_markdown;

/// Name of the markdown file that holds this skill's instructions.
///
/// The file lives under `src/skills/` and is loaded by [`skill_markdown::load`].
const CONTENT_FILE: &str = "roles_and_permissions.md";

/// Skill for managing application roles and permissions across the stack.
pub struct RolesAndPermissionsSkill {
    tags: Vec<String>,
}

impl RolesAndPermissionsSkill {
    pub fn new() -> Self {
        Self {
            tags: vec!["roles", "permissions", "security", "rbac", "backend", "auth"]
                .into_iter()
                .map(str::to_string)
                .collect(),
        }
    }
}

impl Default for RolesAndPermissionsSkill {
    fn default() -> Self {
        Self::new()
    }
}

impl Skill for RolesAndPermissionsSkill {
    fn name(&self) -> &str {
        "roles_and_permissions"
    }

    fn description(&self) -> &str {
        "Teaches the agent how to create roles, add or remove permissions, delete roles, and enforce role-based access control across backend and frontend."
    }

    fn content(&self) -> &str {
        skill_markdown::load(CONTENT_FILE)
    }

    fn category(&self) -> Option<&str> {
        Some("security")
    }

    fn tags(&self) -> &[String] {
        &self.tags
    }
}
