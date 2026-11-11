use crate::llm::{project_shells, App, Shell, Tool};
use crate::tools::{ListDirectoryTool, ReadFileTool, ReadSchemaTool};

/// The Max mode planning agent.
///
/// Max mode is a two-phase orchestrator for large applications. Phase 1 (this
/// agent) breaks a requirement into a list of self-contained tasks — one
/// screen, CRUD, report or dashboard each — and, for every task, records the
/// entity, its fields, the screen/menu wiring and the build steps. Phase 2 runs
/// one sequential implementer agent per task, using that section as its brief.
/// This app only has read-only tools; it never modifies the project.
///
/// The planner writes the plan exactly once, as Markdown. Earlier revisions had
/// it emit the same task list twice (prose + a fenced JSON array) which wasted
/// output tokens and let the two copies drift apart, so the JSON contract is
/// gone and `max::parse_tasks` reads the Markdown sections directly.
///
/// The planning instructions live in the bundled `planning` skill
/// (`src/skills/planning/SKILL.md`), which is compiled into this prompt with
/// `include_str!` (frontmatter stripped). That skill defines how each
/// artifact kind must be described: entities with name and fields, dashboards
/// as concrete widgets, reports as filters and columns, and workflows as
/// actions on existing screens.
///
/// The planner deliberately has no runtime skills: it must not depend on the
/// admin-panel framework guidance, and callers run it with an empty skills
/// list so that no skills catalogue is appended to its system prompt. The
/// `load_skill` tool is not offered; the skill body is embedded directly.
pub struct MaxPlannerApp;

/// Body of the bundled `planning` skill: SKILL.md with its YAML frontmatter
/// removed.
const PLANNING_SKILL_RAW: &str = include_str!("../skills/planning/SKILL.md");

/// Returns the planning skill content with its frontmatter stripped.
///
/// Line endings are normalized first so the split also works when the source
/// file is checked out with CRLF endings.
fn planning_skill_body() -> &'static str {
    static BODY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    BODY.get_or_init(|| {
        let normalized = PLANNING_SKILL_RAW.replace("\r\n", "\n");
        let (_, rest) = normalized.split_once("---\n").unwrap_or(("", &normalized));
        let (_, body) = rest.split_once("\n---\n").unwrap_or(("", rest));
        body.trim_start().to_string()
    })
}

impl App for MaxPlannerApp {
    fn shells(&self) -> Vec<Box<dyn Shell>> {
        project_shells()
    }

    fn tools(&self) -> Vec<Box<dyn Tool>> {
        vec![
            Box::new(ListDirectoryTool::new()),
            Box::new(ReadFileTool::new()),
            Box::new(ReadSchemaTool::new()),
        ]
    }

    fn system_prompt(&self) -> String {
        let planner_rules = r#"You are a project planner for large admin-panel web applications.
Your job is to break the user's requirement down into a list of individual sections that can
each be built independently by a separate implementation agent.

Guidelines:
- One section = one self-contained chunk: one entity/CRUD (with its child line-item entities
  when applicable), one dashboard, one report, or one workflow. Never bundle unrelated
  features into the same section.
- Order the sections the way a human would build them: foundational entities and lists first,
  then actions on them, then reports and dashboards that summarise them.
- Follow the Planning Skill below exactly when describing each kind of section: entities get
  a name and fields, dashboards name the actual KPI cards / charts / tables, reports name
  filters and columns, and workflows name the concrete actions (where the control lives, what
  the user does, which rules apply).
- Define the END RESULT of each section, never the implementation steps. Do not write "sync
  schema", "create controller", "add menu entry" or other build instructions — the
  implementation agent decides how to get there.
- You may inspect the project read-only to understand what already exists, but you do NOT have
  file-modifying tools.
- The section headings in the example plan (`## Overview`, `## N. <title>`) are mandatory:
  the plan is parsed from those headings.
- You do NOT have any skills catalogue or the load_skill tool available; the Planning Skill
  itself is embedded below and is all you need.

Respond with EXACTLY one Markdown document and nothing else. Emit the plan ONCE: do not repeat
it as JSON, YAML, or any other machine-readable block.

=== PLANNING SKILL ===
"#;

        format!("{planner_rules}{}", planning_skill_body())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planner_tools_are_read_only_and_offer_no_skills() {
        let tools = MaxPlannerApp.tools();
        let names: Vec<&str> = tools.iter().map(|tool| tool.name()).collect();
        assert_eq!(names, vec!["list_directory", "read_file", "read_schema"]);
        assert!(!names.contains(&"load_skill"));
    }

    #[test]
    fn planner_prompt_does_not_advertise_skills() {
        let prompt = MaxPlannerApp.system_prompt();
        assert!(!prompt.contains("=== AVAILABLE SKILLS ==="));
        assert!(!prompt.contains("load_skill tool with the skill name"));
        assert!(!prompt.contains("load them with the load_skill tool"));
    }

    #[test]
    fn planner_prompt_embeds_planning_skill() {
        let prompt = MaxPlannerApp.system_prompt();
        // The skill body is embedded with the frontmatter stripped.
        assert!(prompt.contains("=== PLANNING SKILL ==="));
        assert!(prompt.contains("# Planning Skill"));
        assert!(!prompt.contains("name: planning"));
        // The four artifact-kind directives come from the skill.
        assert!(prompt.contains("## Entities"));
        assert!(prompt.contains("## Dashboards"));
        assert!(prompt.contains("## Reports"));
        assert!(prompt.contains("## Workflows"));
        assert!(prompt.contains("## Example Plan"));
        // Format contract that max::parse_tasks depends on.
        assert!(prompt.contains("`## Overview`"));
        assert!(prompt.contains("`## N. <title>`"));
        // Sections define the end result, not build steps.
        assert!(!prompt.contains("### Build Steps"));
        assert!(prompt.contains("never the implementation steps"));
    }

    #[test]
    fn planner_prompt_no_skills_catalogue() {
        let prompt = MaxPlannerApp.system_prompt();
        assert!(!prompt.contains("=== AVAILABLE SKILLS ==="));
        assert!(!prompt.contains("load_skill tool with the skill name"));
        assert!(!prompt.contains("load them with the load_skill tool"));
    }

    #[test]
    fn planner_prompt_no_longer_requests_duplicate_json() {
        let prompt = MaxPlannerApp.system_prompt();
        assert!(!prompt.contains("```json"));
        assert!(!prompt.contains("\"title\":"));
        assert!(prompt.contains("Emit the plan ONCE"));
    }
}
