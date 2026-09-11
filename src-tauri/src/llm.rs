use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::error::Error;
use std::fs;
use std::path::Path;
use std::sync::RwLock;

use crate::progress::ProgressLog;

use crate::tools::{
    LoadSkillTool, MoveFileTool, ReadFileTool, SyncSchemaTool, WriteFileTool, WriteMenuTool,
    WritePageTool,
};

#[path = "symfony.rs"]
mod symfony;

#[path = "react.rs"]
mod react;

#[path = "agent.rs"]
mod agent;
pub use agent::{AgentArtifact, AgentRunResult, AgentService};

#[path = "crud_creation.rs"]
mod crud_creation;
pub use crud_creation::CrudCreationSkill;

#[path = "analytics_and_reporting.rs"]
mod analytics_and_reporting;
pub use analytics_and_reporting::AnalyticsAndReportingSkill;

#[path = "customization_branding.rs"]
mod customization_branding;
pub use customization_branding::CustomizationBrandingSkill;

#[path = "mock_llm.rs"]
mod mock_llm;
pub use mock_llm::MockLlmService;

pub type LlmResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// An event emitted while an agent is running.
pub type AgentEvent = Value;

/// Options for running an agent.
pub struct AgentOptions {
    pub max_turns: Option<u32>,
    pub context: Option<Map<String, Value>>,
    /// The project directory tools must operate on for this agent run.
    pub project_path: Option<String>,
    /// Files the user attached to the message. Only their names reach the
    /// prompt; the contents are never sent to the upstream provider.
    pub artifacts: Vec<AgentArtifact>,
    pub on_event: Option<Box<dyn Fn(AgentEvent) + Send + Sync>>,
    pub log_conversation: bool,
    pub log_dir: Option<String>,
}

impl Default for AgentOptions {
    fn default() -> Self {
        Self {
            max_turns: None,
            context: None,
            project_path: None,
            artifacts: Vec::new(),
            on_event: None,
            log_conversation: true,
            log_dir: None,
        }
    }
}

/// The application definition used for new projects.
pub struct AdminPanelApp;

/// Compatibility name for callers that used the original application name.
pub type AdminPanel = AdminPanelApp;

impl App for AdminPanelApp {
    fn shells(&self) -> Vec<Box<dyn Shell>> {
        vec![
            Box::new(symfony::SymfonyShell::new()),
            Box::new(react::ReactShell::new()),
        ]
    }

    fn tools(&self) -> Vec<Box<dyn Tool>> {
        vec![
            Box::new(LoadSkillTool::new(default_skills())),
            Box::new(ReadFileTool::new()),
            Box::new(WriteFileTool::new()),
            Box::new(MoveFileTool::new()),
            Box::new(WriteMenuTool::new()),
            Box::new(WritePageTool::new()),
            Box::new(SyncSchemaTool::new()),
        ]
    }

    fn skills(&self) -> Vec<Box<dyn Skill>> {
        Vec::new()
    }

    fn system_prompt(&self) -> String {
        "You are a development agent with access to a variety of tools to build powerful admin-panel applications.
        You can only build standard admin-panel web applications consisting of dashboards, reports, forms and a sidebar.
        If the user asks for something else, politely explain the limitations and suggest building an admin panel app, or checking
        back later to see if an update adds it.

        If a user requests something that's not possible from the tools provided to you, explain the situation and tell them that they can
        create an issue report or feature request. Ask them to click the Purple icon in the bottom right, select either \"Report a Bug\" or
        \"Request a Feature\" and share their feedback with the developer.
        
        ".to_string()
    }
}

/// Creates the selected project directory and runs the initial agent setup.
///
/// The shells run one after another rather than side by side, so `log` reaches
/// each of them in turn — the window's frontend tab stays empty until the backend
/// is finished.
pub async fn initialize_project(
    project_path: &str,
    slug: &str,
    template_root: &Path,
    log: &ProgressLog,
    mysql_password: &str,
) -> LlmResult<()> {
    let path = Path::new(project_path);
    fs::create_dir_all(path)?;

    let app = AdminPanelApp;
    let shells = app.shells();

    // Initialize each shell against the project directory. SymfonyShell creates
    // the backend and ReactShell creates the frontend under project_path.
    for shell in &shells {
        if let Err(error) = shell
            .init(project_path, slug, template_root, log, mysql_password)
            .await
        {
            return Err(error);
        }
    }
    Ok(())
}

/// Starts the servers for an already initialized project.
pub async fn start_project(project_path: &str) -> LlmResult<()> {
    let app = AdminPanelApp;
    let shells = app.shells();

    for shell in &shells {
        shell.start(project_path).await?;
    }

    Ok(())
}

/// The role of a message in an LLM conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LLMMessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// A message in an LLM conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMMessage {
    pub role: LLMMessageRole,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_args: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<Value>>,
}

/// Abstract service for executing prompts against an LLM.
#[async_trait]
pub trait LLMService: Send + Sync {
    /// Execute a prompt with the LLM service.
    async fn execute_prompt(
        &self,
        prompt: &str,
        context: Option<&Map<String, Value>>,
    ) -> LlmResult<String>;

    /// Execute a prompt with the tools available in this application.
    async fn execute_prompt_with_tools(
        &self,
        messages: &[LLMMessage],
        tools: &[Box<dyn Tool>],
        context: Option<&Map<String, Value>>,
    ) -> LlmResult<Value>;
}

/// Defines the shells, tools, skills, and system prompt available to the LLM.
pub trait App: Send + Sync {
    fn shells(&self) -> Vec<Box<dyn Shell>>;
    fn tools(&self) -> Vec<Box<dyn Tool>>;
    fn skills(&self) -> Vec<Box<dyn Skill>>;
    fn system_prompt(&self) -> String;
}

/// A shell that can be initialized for a project.
#[async_trait]
pub trait Shell: Send + Sync {
    /// Builds this shell's half of a project, reporting to `log` as it goes. Each
    /// implementation logs under a channel of its own, so the window can tab
    /// between them.
    async fn init(
        &self,
        project_path: &str,
        slug: &str,
        template_root: &Path,
        log: &ProgressLog,
        _mysql_password: &str,
    ) -> LlmResult<()>;
    async fn start(&self, project_path: &str) -> LlmResult<()>;
}

/// Metadata describing a skill available to the LLM.
pub trait Skill: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn content(&self) -> &str;
    fn category(&self) -> Option<&str>;
    fn tags(&self) -> &[String];
}

/// Returns the skills bundled with the application by default.
pub fn default_skills() -> Vec<Box<dyn Skill>> {
    vec![
        Box::new(CrudCreationSkill::default()),
        Box::new(AnalyticsAndReportingSkill::default()),
        Box::new(CustomizationBrandingSkill::default()),
    ]
}

/// A tool available to the LLM, including its parameter schema and executor.
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> &Map<String, Value>;
    async fn execute(&self, args: Value) -> LlmResult<Value>;
}
