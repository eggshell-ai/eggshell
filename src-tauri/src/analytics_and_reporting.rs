use super::Skill;
use super::skill_markdown;

/// Name of the markdown file that holds this skill's instructions.
///
/// The file lives under `src/skills/` and is loaded by [`skill_markdown::load`].
const CONTENT_FILE: &str = "analytics_and_reporting.md";

/// Skill for creating backend analytics aggregators and dashboards.
pub struct AnalyticsAndReportingSkill {
    tags: Vec<String>,
}

impl AnalyticsAndReportingSkill {
    pub fn new() -> Self {
        Self {
            tags: vec!["analytics", "reporting", "dashboard"]
                .into_iter()
                .map(str::to_string)
                .collect(),
        }
    }
}

impl Default for AnalyticsAndReportingSkill {
    fn default() -> Self { Self::new() }
}

impl Skill for AnalyticsAndReportingSkill {
    fn name(&self) -> &str { "analytics_and_reporting" }
    fn description(&self) -> &str {
        "Teaches the agent how to create backend analytics aggregators and dashboards"
    }
    fn content(&self) -> &str { skill_markdown::load(CONTENT_FILE) }
    fn category(&self) -> Option<&str> { Some("analytics") }
    fn tags(&self) -> &[String] { &self.tags }
}
