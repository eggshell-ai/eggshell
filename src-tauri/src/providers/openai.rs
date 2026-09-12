//! The OpenAI-compatible provider: talks to any endpoint that implements the
//! OpenAI Chat Completions API (OpenAI itself, LM Studio, vLLM, OpenRouter,
//! and friends), mirroring the Ollama provider's shape.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::sync::RwLock;

use crate::config::ProviderConfig;
use crate::llm::{LlmResult, LLMMessage, LLMMessageRole, LLMService, Tool};

/// The settings the OpenAI-compatible provider needs, kept separate from the
/// file format for the same reason as `OllamaSettings`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct OpenAiSettings {
    pub model: String,
    #[serde(rename = "apiKey")]
    pub api_key: String,
}

impl From<&ProviderConfig> for OpenAiSettings {
    fn from(config: &ProviderConfig) -> Self {
        Self {
            // The first configured model is the default; the rest are choices.
            model: config.models.first().cloned().unwrap_or_default(),
            api_key: config.api_key.clone(),
        }
    }
}

pub struct OpenAiService {
    client: reqwest::Client,
    api_url: String,
    /// Rewritten in place by the setup screen, so read per request.
    settings: RwLock<OpenAiSettings>,
}

impl OpenAiService {
    pub fn new(config: OpenAiSettings) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_url: "https://api.openai.com/v1".to_string(),
            settings: RwLock::new(config),
        }
    }

    /// Replaces the provider settings used by every later request.
    pub fn apply(&self, config: OpenAiSettings) {
        *self
            .settings
            .write()
            .unwrap_or_else(|poison| poison.into_inner()) = config;
    }

    /// A poisoned lock still guards a whole value, so read it rather than
    /// taking every later prompt down with it.
    fn settings(&self) -> OpenAiSettings {
        self.settings
            .read()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }

    fn request(&self, endpoint: &str, api_key: &str) -> reqwest::RequestBuilder {
        self.client
            .post(format!(
                "{}/{}",
                self.api_url.trim_end_matches('/'),
                endpoint
            ))
            .header("Content-Type", "application/json")
            .bearer_auth(api_key)
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
impl LLMService for OpenAiService {
    async fn execute_prompt(
        &self,
        prompt: &str,
        context: Option<&Map<String, Value>>,
    ) -> LlmResult<String> {
        let settings = self.settings();
        let response = self
            .request("chat/completions", &settings.api_key)
            .json(&serde_json::json!({
                "model": settings.model,
                "messages": [
                    { "role": "user", "content": Self::prompt_with_context(prompt, context) }
                ],
            }))
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        Ok(response["choices"][0]["message"]["content"]
            .as_str()
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
        let mut openai_messages = Vec::with_capacity(messages.len() + 1);
        openai_messages.push(serde_json::json!({
            "role": "system",
            "content": Self::prompt_with_context(&system, context)
        }));
        for message in messages {
            let mut value = serde_json::json!({
                "role": match &message.role {
                    LLMMessageRole::System => "system",
                    LLMMessageRole::User => "user",
                    LLMMessageRole::Assistant => "assistant",
                    LLMMessageRole::Tool => "tool",
                },
                "content": message.content,
            });
            if let Some(tool_calls) = &message.tool_calls {
                value["tool_calls"] = Value::Array(tool_calls.clone());
            }
            openai_messages.push(value);
        }
        let tool_definitions = tools
            .iter()
            .map(|tool| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": tool.name(),
                        "description": tool.description(),
                        "parameters": tool.parameters(),
                    }
                })
            })
            .collect::<Vec<_>>();
        let settings = self.settings();
        let response = self
            .request("chat/completions", &settings.api_key)
            .json(&serde_json::json!({
                "model": settings.model,
                "messages": openai_messages,
                "tools": tool_definitions,
            }))
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        let message = response
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .cloned()
            .unwrap_or_default();
        let calls = message
            .get("tool_calls")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(serde_json::json!({
            "content": message.get("content").and_then(Value::as_str).unwrap_or_default(),
            "tool_calls": calls,
        }))
    }

    /// The OpenAI-compatible `GET /models` endpoint answers with
    /// `{ "data": [{ "id": ... }] }`; OpenRouter, LM Studio and vLLM mirror it.
    async fn list_models(&self) -> LlmResult<Vec<String>> {
        let settings = self.settings();
        let response = self
            .client
            .get(format!("{}/models", self.api_url.trim_end_matches('/')))
            .bearer_auth(&settings.api_key)
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        let models = response
            .get("data")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| entry.get("id").and_then(Value::as_str))
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(models)
    }
}
