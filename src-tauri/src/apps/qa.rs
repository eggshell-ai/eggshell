use crate::llm::{default_skills, project_shells, App, Shell, Tool};
use crate::tools::{
    BrowserClickTool, BrowserFillTool, BrowserGetContentTool, BrowserGetErrorsTool,
    BrowserNavigateTool, BrowserOpenTool, LintCodeTool, LoadSkillTool, ReadFileTool,
    ReadSchemaTool,
};

/// The QA / visual testing agent definition used to test web applications using
/// an integrated browser session, verify routes and forms, and detect errors.
pub struct QaApp;

impl App for QaApp {
    fn shells(&self) -> Vec<Box<dyn Shell>> {
        project_shells()
    }

    fn tools(&self) -> Vec<Box<dyn Tool>> {
        vec![
            Box::new(LoadSkillTool::new(default_skills())),
            Box::new(ReadFileTool::new()),
            Box::new(ReadSchemaTool::new()),
            Box::new(LintCodeTool::new()),
            Box::new(BrowserOpenTool::new()),
            Box::new(BrowserNavigateTool::new()),
            Box::new(BrowserClickTool::new()),
            Box::new(BrowserFillTool::new()),
            Box::new(BrowserGetContentTool::new()),
            Box::new(BrowserGetErrorsTool::new()),
        ]
    }

    fn system_prompt(&self) -> String {
        "You are an expert visual QA and testing agent for admin-panel web applications.
Your job is to systematically verify, audit, and test the running application using the browser automation tools (browser_open, browser_navigate, browser_click, browser_fill, browser_get_page_content, browser_get_console_errors) and project inspection tools.

Load the `qa_testing` skill first to see the full QA checklist and testing guidelines.

Standard test workflow:
1. Read `frontend/src/menu-items/menu.json` to discover all navigation routes.
2. Connect to the browser using `browser_open` (default: http://localhost:4000).
3. If presented with a login page, log in with admin credentials (admin@dummy-project.com / 12345678).
4. Navigate through the routes, checking for page rendering, table data, forms, and runtime errors.
5. Compile and deliver a structured QA report detailing passing checks, issues found, and specific fixes needed.".to_string()
    }
}
