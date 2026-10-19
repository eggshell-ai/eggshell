use crate::llm::{default_skills, project_shells, App, Shell, Tool};
use crate::tools::{LoadSkillTool, ReadFileTool, ReadSchemaTool};

/// The planning agent definition used to design entities, reports, requirements,
/// and implementation approach without performing file writes or mutations.
pub struct PlanningApp;

impl App for PlanningApp {
    fn shells(&self) -> Vec<Box<dyn Shell>> {
        project_shells()
    }

    fn tools(&self) -> Vec<Box<dyn Tool>> {
        vec![
            Box::new(LoadSkillTool::new(default_skills())),
            Box::new(ReadFileTool::new()),
            Box::new(ReadSchemaTool::new()),
        ]
    }

    fn system_prompt(&self) -> String {
        "You are an expert software architect and planning agent for admin-panel web applications.
Your job is to analyze the user's requirements and produce a structured, thorough implementation plan in Markdown.

In this mode, all file-modifying tools (write_file, move_file, patch_file, sync_schema) are DISABLED.
You only have read access to inspect the project if needed. DO NOT attempt to write or execute code changes.

Your plan MUST be formatted with clear Markdown headings and cover:
1. **Overview & Requirements**: Summary of the requested app/feature and key user workflows.
2. **Entities & Database Schema**: Detailed entities, their fields (types, constraints), and relationships (One-to-Many, Many-to-Many).
3. **Reports & Dashboards**: Summary metrics, aggregations, charts, and report views needed.
4. **Navigation & Menu Structure**: Sidebar menu items, routes, and page layout.
5. **Implementation Steps & Approach**: Logical step-by-step breakdown of backend controllers, frontend pages, and schema migrations to execute once the user proceeds.

Be concise yet comprehensive, structured, and ready for execution.".to_string()
    }
}
