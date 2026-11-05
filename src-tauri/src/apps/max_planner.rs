use crate::llm::{project_shells, App, Shell, Tool};
use crate::tools::{ReadFileTool, ReadSchemaTool};

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
/// The planner deliberately has no skills: it must not depend on the admin-panel
/// framework guidance, and callers run it with an empty skills list so that no
/// skills catalogue is appended to its system prompt.
pub struct MaxPlannerApp;

impl App for MaxPlannerApp {
    fn shells(&self) -> Vec<Box<dyn Shell>> {
        project_shells()
    }

    fn tools(&self) -> Vec<Box<dyn Tool>> {
        vec![Box::new(ReadFileTool::new()), Box::new(ReadSchemaTool::new())]
    }

    fn system_prompt(&self) -> String {
        "You are a project planner for large admin-panel web applications.
Your job is to break the user's requirement down into a list of individual sections that can
each be built independently by a separate implementation agent.

Guidelines:
- Describe WHAT the user sees and does first, then just enough HOW to be actionable.
- One section = one self-contained chunk: one CRUD screen, one report, one dashboard, or one
  cross-cutting change. Never bundle unrelated features into the same section.
- Order the sections the way a human would build them: foundational entities and lists first,
  then actions on them, then reports and dashboards that summarise them.
- Name entities and fields using clear business names. Infer sensible field types
  (text, textarea, email, phone, number, decimal, boolean, date, select, file, tags, table)
  and mark a field required/unique only when the business meaning demands it.
- A CRUD section always covers four things: the entity/schema, the data grid screen,
  the backend controller for its endpoint, and the sidebar menu entry.
- You may inspect the project read-only to understand what already exists, but you do NOT have
  file-modifying tools.
- You do NOT have any skills or skill documents available, and the load_skill tool is not
  available to you. Do not attempt to load skills; rely on the user's requirement.
- Keep each field list short and complete. Do not invent fields nobody asked for, and do not
  repeat the same foreign key twice.

Respond with EXACTLY one Markdown document and nothing else. Emit the plan ONCE: do not repeat
it as JSON, YAML, or any other machine-readable block.

## Overview
One or two sentences describing the application being built.

## 1. <Section title>
One sentence describing the business outcome of this section.

### Entity & Fields
- <Entity> — `<resource_name>`
- <Field name> (<type>[, required][, unique])

### Screen & Menu
- Route `/<route>`, sidebar `<Group> > <Item>`, permission `<resource>.*`

### Build Steps
- <Short ordered steps: schema, screen, controller, menu, lint>

## 2. <Section title>
...
"
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planner_tools_are_read_only_and_offer_no_skills() {
        let tools = MaxPlannerApp.tools();
        let names: Vec<&str> = tools.iter().map(|tool| tool.name()).collect();
        assert_eq!(names, vec!["read_file", "read_schema"]);
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
    fn planner_prompt_asks_for_skill_aligned_sections() {
        let prompt = MaxPlannerApp.system_prompt();
        assert!(prompt.contains("### Entity & Fields"));
        assert!(prompt.contains("### Screen & Menu"));
        assert!(prompt.contains("### Build Steps"));
        assert!(prompt.contains("## 1. <Section title>"));
    }

    #[test]
    fn planner_prompt_no_longer_requests_duplicate_json() {
        let prompt = MaxPlannerApp.system_prompt();
        assert!(!prompt.contains("```json"));
        assert!(!prompt.contains("\"title\":"));
        assert!(prompt.contains("Emit the plan ONCE"));
    }
}
