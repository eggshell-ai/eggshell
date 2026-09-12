//! The Ollama provider: the concrete `LLMService` implementation, extracted
//! from `llm.rs` so `llm.rs` stays provider-agnostic.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::error::Error;
use std::sync::RwLock;

use crate::config::ProviderConfig;
use crate::llm::{LlmResult, LLMMessage, LLMService, Tool};

/// The settings Ollama needs, kept separate from the file format so the
/// provider owns its own shape and the config file can evolve independently.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct OllamaSettings {
    pub model: String,
    #[serde(rename = "apiKey")]
    pub api_key: String,
}

impl From<&ProviderConfig> for OllamaSettings {
    fn from(config: &ProviderConfig) -> Self {
        Self {
            // The first configured model is the default; the rest are choices.
            model: config.models.first().cloned().unwrap_or_default(),
            api_key: config.api_key.clone(),
        }
    }
}

pub struct OllamaService {
    client: reqwest::Client,
    api_url: String,
    /// The setup screen can rewrite the model and key while Eggshell is running,
    /// so these are read per request instead of being fixed at start-up.
    settings: RwLock<OllamaSettings>,
}

impl OllamaService {
    pub fn new(config: OllamaSettings) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_url: "https://ollama.com/api".to_string(),
            settings: RwLock::new(config),
        }
    }

    /// Replaces the provider settings used by every later request.
    pub fn apply(&self, config: OllamaSettings) {
        *self
            .settings
            .write()
            .unwrap_or_else(|poison| poison.into_inner()) = config;
    }

    /// A poisoned lock only means an earlier writer panicked partway through;
    /// the settings behind it are still a whole value, so read them rather than
    /// taking every later prompt down with it.
    fn settings(&self) -> OllamaSettings {
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
                "role": match &message.role { crate::llm::LLMMessageRole::System => "system", crate::llm::LLMMessageRole::User => "user", crate::llm::LLMMessageRole::Assistant => "assistant", crate::llm::LLMMessageRole::Tool => "tool" },
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

    /// Ollama exposes its catalogue over `GET /api/tags`, which needs no model
    /// and answers with `{ "models": [{ "name": ... }] }` — both for a local
    /// daemon and for ollama.com.
    async fn list_models(&self) -> LlmResult<Vec<String>> {
        let settings = self.settings();
        let mut request = self.client.get(format!(
            "{}/tags",
            self.api_url.trim_end_matches('/')
        ));
        if !settings.api_key.is_empty() && settings.api_key != "..." {
            request = request.bearer_auth(&settings.api_key);
        }
        let response = request
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        let models = response
            .get("models")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| entry.get("name").and_then(Value::as_str))
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(models)
    }
}

/// Kept for callers that still name the error type through the provider module.
pub type BoxError = Box<dyn Error + Send + Sync>;
