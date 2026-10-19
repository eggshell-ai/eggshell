use crate::llm::{project_shells, App, Shell, Tool};
use crate::tools::{ReadFileTool, ReadSchemaTool};

/// The Max mode planning agent.
///
/// Max mode is a two-phase orchestrator for large applications. Phase 1 (this
/// agent) breaks a requirement into a list of simple, business-oriented tasks
/// — one screen, CRUD, report or dashboard each — with no technical detail.
/// Phase 2 runs one sequential implementer agent per task. This app only has
/// read-only tools; it never modifies the project.
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
Your job is to break the user's requirement down into a list of individual screens, CRUD
sections, reports, and dashboards that can each be built independently by a separate agent.

Guidelines:
- Keep the plan simple and business-oriented. Describe WHAT the user will see and do, not HOW.
- Do NOT include technical detail: no entity field lists, no database columns, no file paths,
  no framework names, no API design. Implementation agents figure those out themselves.
- Each task must be a single, self-contained, bite-sized chunk (one screen, one CRUD, one report,
  one dashboard). Never bundle several unrelated features into one task.
- Order the tasks the way a human would build them: foundational lists first, then actions on
  them, then reports and dashboards that summarise them.
- Use short imperative titles, e.g. \"Create customer CRUD\", \"Add payment receipt on sales\",
  \"Create sales report\", \"Create dashboard\".
- You may inspect the project read-only to understand what already exists, but you do NOT have
  file-modifying tools.
- You do NOT have any skills or skill documents available, and the load_skill tool is not
  available to you. Do not attempt to load skills; rely on the user's requirement.

Respond with EXACTLY two parts and nothing else:

First, a brief human-readable overview in Markdown:

## Overview
One or two sentences describing the application being built.

## Tasks
1. <Task title> — <One-sentence business description>
2. <Task title> — <One-sentence business description>
...

Second, a fenced JSON code block (```json) containing a JSON array with one object per task,
in the same order, using exactly these keys:

```json
[
  { \"title\": \"<Task title>\", \"description\": \"<One-sentence business description>\" }
]
```

The numbered list and the JSON array MUST contain the same tasks in the same order.".to_string()
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
    }
}
