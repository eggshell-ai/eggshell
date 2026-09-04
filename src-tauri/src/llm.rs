use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::error::Error;
use std::fs;
use std::path::Path;
use std::sync::RwLock;

use crate::config::OllamaConfig;
use crate::progress::ProgressLog;

use crate::tools::{
    LoadSkillTool, ReadFileTool, SyncSchemaTool, WriteFileTool, WriteMenuTool, WritePageTool,
};

#[path = "symfony.rs"]
mod symfony;

#[path = "react.rs"]
mod react;

#[path = "agent.rs"]
mod agent;
pub use agent::{AgentRunResult, AgentService};

#[path = "crud_creation.rs"]
mod crud_creation;
pub use crud_creation::CrudCreationSkill;

#[path = "analytics_and_reporting.rs"]
mod analytics_and_reporting;
pub use analytics_and_reporting::AnalyticsAndReportingSkill;

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
        back later to see if an update adds it.".to_string()
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

pub struct OllamaService {
    client: reqwest::Client,
    api_url: String,
    /// The setup screen can rewrite the model and key while Eggshell is running,
    /// so these are read per request instead of being fixed at start-up.
    settings: RwLock<OllamaConfig>,
}

impl OllamaService {
    pub fn new(config: OllamaConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_url: "https://ollama.com/api".to_string(),
            settings: RwLock::new(config),
        }
    }

    /// Replaces the provider settings used by every later request.
    pub fn apply(&self, config: OllamaConfig) {
        *self
            .settings
            .write()
            .unwrap_or_else(|poison| poison.into_inner()) = config;
    }

    /// A poisoned lock only means an earlier writer panicked partway through;
    /// the settings behind it are still a whole value, so read them rather than
    /// taking every later prompt down with it.
    fn settings(&self) -> OllamaConfig {
        self.settings
            .read()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }

    fn request(&self, endpoint: &str, api_key: &str) -> reqwest::RequestBuilder {
        let request = self
            .client
            .post(format!(
                "{}/{}",
                self.api_url.trim_end_matches('/'),
                endpoint
            ))
            .header("Content-Type", "application/json");
        if api_key.is_empty() || api_key == "..." {
            request
        } else {
            request.bearer_auth(api_key)
        }
    }

    fn prompt_with_context(prompt: &str, context: Option<&Map<String, Value>>) -> String {
        let Some(context) = context.filter(|value| !value.is_empty()) else {
            return prompt.to_string();
        };
        let context = context
            .iter()
            .map(|(key, value)| format!("{}: {}", key, value))
            .collect::<Vec<_>>()
            .join("\n");
        format!("Context:\n{context}\n\n{prompt}")
    }
}

#[async_trait]
impl LLMService for OllamaService {
    async fn execute_prompt(
        &self,
        prompt: &str,
        context: Option<&Map<String, Value>>,
    ) -> LlmResult<String> {
        let settings = self.settings();
        let response = self
            .request("generate", &settings.api_key)
            .json(&serde_json::json!({
                "model": settings.model,
                "prompt": Self::prompt_with_context(prompt, context),
                "stream": false,
            }))
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        Ok(response["response"]
            .as_str()
            .or_else(|| response["message"]["content"].as_str())
            .unwrap_or_default()
            .to_string())
    }

    async fn execute_prompt_with_tools(
        &self,
        messages: &[LLMMessage],
        tools: &[Box<dyn Tool>],
        context: Option<&Map<String, Value>>,
    ) -> LlmResult<Value> {
        let system = format!(
            "You are an AI assistant with access to these tools:\n{}",
            tools
                .iter()
                .map(|tool| format!("- {}: {}", tool.name(), tool.description()))
                .collect::<Vec<_>>()
                .join("\n")
        );
        let mut ollama_messages = Vec::with_capacity(messages.len() + 1);
        ollama_messages.push(serde_json::json!({ "role": "system", "content": Self::prompt_with_context(&system, context) }));
        for message in messages {
            let mut value = serde_json::json!({
                "role": match &message.role { LLMMessageRole::System => "system", LLMMessageRole::User => "user", LLMMessageRole::Assistant => "assistant", LLMMessageRole::Tool => "tool" },
                "content": message.content,
            });
            if let Some(tool_calls) = &message.tool_calls {
                value["tool_calls"] = Value::Array(tool_calls.clone());
            }
            ollama_messages.push(value);
        }
        let tool_definitions = tools.iter().map(|tool| serde_json::json!({ "type": "function", "function": { "name": tool.name(), "description": tool.description(), "parameters": tool.parameters() } })).collect::<Vec<_>>();
        let settings = self.settings();
        let response = self.request("chat", &settings.api_key).json(&serde_json::json!({ "model": settings.model, "messages": ollama_messages, "stream": false, "tools": tool_definitions })).send().await?.error_for_status()?.json::<Value>().await?;
        let message = response.get("message").cloned().unwrap_or_default();
        let calls = message
            .get("tool_calls")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(
            serde_json::json!({ "content": message.get("content").and_then(Value::as_str).unwrap_or_default(), "tool_calls": calls }),
        )
    }
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
