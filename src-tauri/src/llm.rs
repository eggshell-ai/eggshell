use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::error::Error;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use crate::progress::ProgressLog;

#[path = "symfony.rs"]
pub(crate) mod symfony;

#[path = "react.rs"]
mod react;

pub use crate::apps::{AdminPanel, AdminPanelApp, MaxPlannerApp, PlanningApp, QaApp};

/// The shells every to-be-built admin-panel project needs: the Symfony backend
/// and the React frontend.
pub(crate) fn project_shells() -> Vec<Box<dyn Shell>> {
    vec![
        Box::new(symfony::SymfonyShell::new()),
        Box::new(react::ReactShell::new()),
    ]
}

#[path = "agent.rs"]
mod agent;
pub use agent::{AgentArtifact, AgentRunResult, AgentService};

#[path = "skill_markdown.rs"]
mod skill_markdown;

#[path = "crud_creation.rs"]
mod crud_creation;
pub use crud_creation::CrudCreationSkill;

#[path = "analytics_and_reporting.rs"]
mod analytics_and_reporting;
pub use analytics_and_reporting::AnalyticsAndReportingSkill;

#[path = "customization_branding.rs"]
mod customization_branding;
pub use customization_branding::CustomizationBrandingSkill;

#[path = "notification_creation.rs"]
mod notification_creation;
pub use notification_creation::NotificationCreationSkill;

#[path = "profile_management.rs"]
mod profile_management;
pub use profile_management::ProfileManagementSkill;

#[path = "roles_and_permissions.rs"]
mod roles_and_permissions;
pub use roles_and_permissions::RolesAndPermissionsSkill;

#[path = "qa_testing.rs"]
mod qa_testing;
pub use qa_testing::QaTestingSkill;

#[path = "mock_llm.rs"]
mod mock_llm;
pub use mock_llm::MockLlmService;

pub type LlmResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Incremental delta streamed from an LLM during completion.
#[derive(Debug, Clone)]
pub enum StreamChunk {
    ThoughtDelta(String),
    ContentDelta(String),
}

pub type StreamCallback = Arc<dyn Fn(StreamChunk) + Send + Sync>;

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
    pub on_event: Option<Arc<dyn Fn(AgentEvent) + Send + Sync>>,
    pub log_conversation: bool,
    pub log_dir: Option<String>,
    pub cancellation_token: Option<Arc<std::sync::atomic::AtomicBool>>,
    /// Skills advertised in the prompt for this run. `None` uses the agent's
    /// default catalogue; `Some(Vec::new())` appends no skills section at all,
    /// which is how mode-specific agents opt out (e.g. the Max planner).
    pub skills: Option<Vec<Box<dyn Skill>>>,
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
            cancellation_token: None,
            skills: None,
        }
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
    is_mariadb: bool,
) -> LlmResult<()> {
    let path = Path::new(project_path);
    fs::create_dir_all(path)?;

    let app = AdminPanelApp;
    let shells = app.shells();

    // Initialize each shell against the project directory. SymfonyShell creates
    // the backend and ReactShell creates the frontend under project_path.
    for shell in &shells {
        if let Err(error) = shell
            .init(project_path, slug, template_root, log, mysql_password, is_mariadb)
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

    /// Execute a prompt with tools while checking for cancellation and optionally streaming deltas.
    async fn execute_prompt_with_tools_cancellable(
        &self,
        messages: &[LLMMessage],
        tools: &[Box<dyn Tool>],
        context: Option<&Map<String, Value>>,
        _cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
        _on_chunk: Option<StreamCallback>,
    ) -> LlmResult<Value> {
        self.execute_prompt_with_tools(messages, tools, context).await
    }

    /// Fetch the models this provider currently offers, using the credentials it
    /// was configured with. Providers that can enumerate their models override
    /// this; the default is an empty list, which the caller treats as "no models
    /// to discover" rather than an error.
    async fn list_models(&self) -> LlmResult<Vec<String>> {
        Ok(Vec::new())
    }
}

/// Defines the shells, tools, and system prompt available to the LLM.
///
/// Skills are not part of a mode's definition: a run selects them through
/// [`AgentOptions::skills`] so a mode can opt out of the catalogue entirely.
pub trait App: Send + Sync {
    fn shells(&self) -> Vec<Box<dyn Shell>>;
    fn tools(&self) -> Vec<Box<dyn Tool>>;
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
        _is_mariadb: bool,
    ) -> LlmResult<()>;
    async fn start(&self, project_path: &str) -> LlmResult<()>;
}

/// An on-demand document attached to a skill.
///
/// Documents let a skill keep its main instructions concise while still
/// shipping deeper reference material that the agent only needs occasionally
/// (for example detailed row-action or custom-UI guidance). Each document is
/// fetched individually through the `load_skill` tool by passing its `name`.
#[derive(Debug, Clone)]
pub struct SkillDocument {
    /// Short identifier used to request the document (e.g. "row_actions").
    pub name: &'static str,
    /// Human-readable description shown in the skills summary.
    pub description: &'static str,
    /// The document body, typically a markdown file embedded at compile time.
    pub content: &'static str,
}

/// Metadata describing a skill available to the LLM.
pub trait Skill: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn content(&self) -> &str;
    fn category(&self) -> Option<&str>;
    fn tags(&self) -> &[String];

    /// Additional documents bundled with this skill, fetched on demand via the
    /// `load_skill` tool. Defaults to an empty list.
    fn documents(&self) -> &[SkillDocument] {
        &[]
    }
}

/// Returns the skills bundled with the application by default.
pub fn default_skills() -> Vec<Box<dyn Skill>> {
    vec![
        Box::new(CrudCreationSkill::default()),
        Box::new(AnalyticsAndReportingSkill::default()),
        Box::new(CustomizationBrandingSkill::default()),
        Box::new(NotificationCreationSkill::default()),
        Box::new(ProfileManagementSkill::default()),
        Box::new(RolesAndPermissionsSkill::default()),
        Box::new(QaTestingSkill::default()),
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
