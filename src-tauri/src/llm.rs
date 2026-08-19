use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::error::Error;
use std::fs;
use std::path::Path;

use crate::config::OllamaConfig;

use crate::tools::{LoadSkillTool, ReadFileTool, SyncSchemaTool, WriteFileTool, WriteMenuTool, WritePageTool};

#[path = "symfony.rs"]
mod symfony;

#[path = "react.rs"]
mod react;

#[path = "agent.rs"]
mod agent;
pub use agent::{AgentRunResult, AgentService};

pub type LlmResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// An event emitted while an agent is running.
pub type AgentEvent = Value;

/// Options for running an agent.
pub struct AgentOptions {
    pub max_turns: Option<u32>,
    pub context: Option<Map<String, Value>>,
    pub on_event: Option<Box<dyn Fn(AgentEvent) + Send + Sync>>,
    pub log_conversation: bool,
    pub log_dir: Option<String>,
}

impl Default for AgentOptions {
    fn default() -> Self {
        Self {
            max_turns: None,
            context: None,
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
        "You are a helpful agent".to_string()
    }
}

/// Creates the selected project directory and runs the initial agent setup.
pub async fn initialize_project(project_path: &str, _slug: &str) -> LlmResult<()> {
    let path = Path::new(project_path);
    fs::create_dir_all(path)?;

    let app = AdminPanelApp;
    let shells = app.shells();

    // Initialize each shell against the project directory. SymfonyShell creates
    // the backend and ReactShell creates the frontend under project_path.
    for shell in &shells {
        if let Err(error) = shell.init(project_path).await {
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

/// Temporary local implementation used while the production LLM client is
/// being built. It deliberately ignores the supplied conversation and returns
/// a deterministic response, but exercises the exact same AgentService path.
pub struct MockLlmService;

pub struct OllamaService {
    client: reqwest::Client,
    api_url: String,
    api_key: String,
    model: String,
}

impl OllamaService {
    pub fn new(config: OllamaConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_url: "https://ollama.com/api".to_string(),
            api_key: config.api_key,
            model: config.model,
        }
    }

    fn request(&self, endpoint: &str) -> reqwest::RequestBuilder {
        let request = self
            .client
            .post(format!(
                "{}/{}",
                self.api_url.trim_end_matches('/'),
                endpoint
            ))
            .header("Content-Type", "application/json");
        if self.api_key.is_empty() || self.api_key == "..." {
            request
        } else {
            request.bearer_auth(&self.api_key)
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
        let response = self
            .request("generate")
            .json(&serde_json::json!({
                "model": self.model,
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
        let response = self.request("chat").json(&serde_json::json!({ "model": self.model, "messages": ollama_messages, "stream": false, "tools": tool_definitions })).send().await?.error_for_status()?.json::<Value>().await?;
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

#[async_trait]
impl LLMService for MockLlmService {
    async fn execute_prompt(
        &self,
        _prompt: &str,
        _context: Option<&Map<String, Value>>,
    ) -> LlmResult<String> {
        Ok("Pong".to_string())
    }

    async fn execute_prompt_with_tools(
        &self,
        _messages: &[LLMMessage],
        _tools: &[Box<dyn Tool>],
        _context: Option<&Map<String, Value>>,
    ) -> LlmResult<Value> {
        Ok(serde_json::json!({ "content": "Pong", "tool_calls": [] }))
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
    async fn init(&self, project_path: &str) -> LlmResult<()>;
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

/// Skill for creating a complete CRUD interface across the application stack.
pub struct CrudCreationSkill {
    tags: Vec<String>,
}

impl CrudCreationSkill {
    pub fn new() -> Self {
        Self {
            tags: vec![
                "crud".to_string(),
                "database".to_string(),
                "frontend".to_string(),
                "backend".to_string(),
                "symfony".to_string(),
                "react".to_string(),
            ],
        }
    }
}

impl Default for CrudCreationSkill {
    fn default() -> Self {
        Self::new()
    }
}

impl Skill for CrudCreationSkill {
    fn name(&self) -> &str {
        "crud_creation"
    }

    fn description(&self) -> &str {
        "Teaches the agent how to create a complete CRUD interface with database schema, backend controller, frontend pages, and menu integration."
    }

    fn content(&self) -> &str {
        ""
    }

    fn category(&self) -> Option<&str> {
        Some("development")
    }

    fn tags(&self) -> &[String] {
        &self.tags
    }
}

/// Skill for creating backend analytics aggregators and dashboards.
pub struct AnalyticsAndReportingSkill {
    tags: Vec<String>,
}

impl AnalyticsAndReportingSkill {
    pub fn new() -> Self {
        Self {
            tags: vec![
                "analytics".to_string(),
                "reporting".to_string(),
                "dashboard".to_string(),
            ],
        }
    }
}

impl Default for AnalyticsAndReportingSkill {
    fn default() -> Self {
        Self::new()
    }
}

impl Skill for AnalyticsAndReportingSkill {
    fn name(&self) -> &str {
        "analytics_and_reporting"
    }

    fn description(&self) -> &str {
        "Teaches the agent how to create backend analytics aggregators and dashboards"
    }

    fn content(&self) -> &str {
        ""
    }

    fn category(&self) -> Option<&str> {
        Some("analytics")
    }

    fn tags(&self) -> &[String] {
        &self.tags
    }
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
