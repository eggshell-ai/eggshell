//! The OpenAI-compatible provider: talks to any endpoint that implements the
//! OpenAI Chat Completions API (OpenAI itself, LM Studio, vLLM, OpenRouter,
//! and friends), mirroring the Ollama provider's shape.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use crate::config::ProviderConfig;
use crate::llm::{LlmResult, LLMMessage, LLMMessageRole, LLMService, Tool};

/// The settings the OpenAI-compatible provider needs, kept separate from the
/// file format for the same reason as `OllamaSettings`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenAiSettings {
    pub model: String,
    #[serde(rename = "apiKey")]
    pub api_key: String,
    #[serde(rename = "baseUrl", default)]
    pub base_url: Option<String>,
    pub reasoning: Option<String>,
}

impl Default for OpenAiSettings {
    fn default() -> Self {
        Self {
            model: String::new(),
            api_key: String::new(),
            base_url: None,
            reasoning: None,
        }
    }
}

impl From<&ProviderConfig> for OpenAiSettings {
    fn from(config: &ProviderConfig) -> Self {
        Self {
            // The first configured model is the default; the rest are choices.
            model: config.models.first().cloned().unwrap_or_default(),
            api_key: config.api_key.clone(),
            base_url: config.base_url.clone().filter(|url| !url.trim().is_empty()),
            reasoning: config.reasoning.clone(),
        }
    }
}

pub struct OpenAiService {
    client: reqwest::Client,
    /// Rewritten in place by the setup screen, so read per request.
    settings: RwLock<OpenAiSettings>,
}

impl OpenAiService {
    pub fn new(config: OpenAiSettings) -> Self {
        Self {
            client: reqwest::Client::new(),
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

    fn api_url(settings: &OpenAiSettings) -> String {
        settings
            .base_url
            .as_deref()
            .filter(|url| !url.trim().is_empty())
            .unwrap_or("https://api.openai.com/v1")
            .trim_end_matches('/')
            .to_string()
    }

    fn request(&self, endpoint: &str, settings: &OpenAiSettings) -> reqwest::RequestBuilder {
        let base_url = Self::api_url(settings);
        self.client
            .post(format!("{}/{}", base_url, endpoint))
            .header("Content-Type", "application/json")
            .bearer_auth(&settings.api_key)
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
            .request("chat/completions", &settings)
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
        self.execute_prompt_with_tools_cancellable(messages, tools, context, None, None).await
    }

    async fn execute_prompt_with_tools_cancellable(
        &self,
        messages: &[LLMMessage],
        tools: &[Box<dyn Tool>],
        context: Option<&Map<String, Value>>,
        cancel: Option<Arc<AtomicBool>>,
        on_chunk: Option<crate::llm::StreamCallback>,
    ) -> LlmResult<Value> {
        if cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
            return Err("Execution interrupted by user.".into());
        }
        let tools_system = format!(
            "You are an AI assistant with access to these tools:\n{}",
            tools
                .iter()
                .map(|tool| format!("- {}: {}", tool.name(), tool.description()))
                .collect::<Vec<_>>()
                .join("\n")
        );
        let mut system_parts = vec![tools_system];
        for message in messages {
            if matches!(message.role, LLMMessageRole::System) && !message.content.trim().is_empty() {
                system_parts.push(message.content.clone());
            }
        }
        let system = system_parts.join("\n\n");

        let mut openai_messages = Vec::with_capacity(messages.len() + 1);
        openai_messages.push(serde_json::json!({
            "role": "system",
            "content": Self::prompt_with_context(&system, context)
        }));
        for message in messages {
            if matches!(message.role, LLMMessageRole::System) {
                continue;
            }
            let mut value = serde_json::json!({
                "role": match &message.role {
                    LLMMessageRole::System => unreachable!(),
                    LLMMessageRole::User => "user",
                    LLMMessageRole::Assistant => "assistant",
                    LLMMessageRole::Tool => "tool",
                },
                "content": message.content,
            });
            if let Some(tool_calls) = &message.tool_calls {
                let normalized_calls = tool_calls
                    .iter()
                    .map(|call| {
                        let mut call = call.clone();
                        if let Some(function) = call.get_mut("function").and_then(Value::as_object_mut) {
                            if let Some(args) = function.get("arguments") {
                                if !args.is_string() {
                                    let serialized = match args {
                                        Value::Null => "{}".to_string(),
                                        other => other.to_string(),
                                    };
                                    function.insert("arguments".to_string(), Value::String(serialized));
                                }
                            }
                        } else if let Some(args) = call.get("arguments") {
                            // In case the tool call had top-level arguments instead of nested under function
                            let serialized = match args {
                                Value::String(s) => s.clone(),
                                Value::Null => "{}".to_string(),
                                other => other.to_string(),
                            };
                            let name = call.get("name").cloned().unwrap_or(Value::String("tool".to_string()));
                            if let Some(call_obj) = call.as_object_mut() {
                                call_obj.insert(
                                    "function".to_string(),
                                    serde_json::json!({
                                        "name": name,
                                        "arguments": serialized,
                                    }),
                                );
                            }
                        }
                        call
                    })
                    .collect::<Vec<_>>();
                value["tool_calls"] = Value::Array(normalized_calls);
            }
            if let Some(call_id) = &message.tool_call_id {
                value["tool_call_id"] = Value::String(call_id.clone());
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
        let mut payload = serde_json::json!({
            "model": settings.model,
            "messages": openai_messages,
            "tools": tool_definitions,
            "stream": true,
        });
        if let Some(reasoning) = &settings.reasoning {
            let reasoning = reasoning.to_lowercase();
            if reasoning != "off" {
                payload["reasoning_effort"] = Value::String(reasoning);
            }
        }

        let send_future = self
            .request("chat/completions", &settings)
            .json(&payload)
            .send();

        let response = if let Some(cancel_token) = &cancel {
            let cancel_token = cancel_token.clone();
            let cancel_watcher = async move {
                while !cancel_token.load(Ordering::Relaxed) {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            };
            tokio::select! {
                res = send_future => res?.error_for_status()?,
                _ = cancel_watcher => {
                    return Err("Execution interrupted by user.".into());
                }
            }
        } else {
            send_future.await?.error_for_status()?
        };

        use eventsource_stream::Eventsource;
        use tokio_stream::StreamExt;

        let mut event_stream = response.bytes_stream().eventsource();
        let mut raw_content = String::new();
        let mut thought = String::new();
        let mut tool_calls_map: std::collections::BTreeMap<usize, (Option<String>, String, String)> = std::collections::BTreeMap::new();

        while let Some(event_result) = event_stream.next().await {
            if cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
                return Err("Execution interrupted by user.".into());
            }
            let event = match event_result {
                Ok(ev) => ev,
                Err(err) => return Err(err.into()),
            };
            if event.data == "[DONE]" {
                break;
            }
            let chunk: Value = match serde_json::from_str(&event.data) {
                Ok(val) => val,
                Err(_) => continue,
            };

            let Some(choices) = chunk.get("choices").and_then(Value::as_array) else {
                continue;
            };
            for choice in choices {
                let Some(delta) = choice.get("delta") else {
                    continue;
                };

                // Extract reasoning delta if present
                if let Some(r_delta) = delta
                    .get("reasoning_content")
                    .or_else(|| delta.get("thought"))
                    .and_then(Value::as_str)
                {
                    if !r_delta.is_empty() {
                        thought.push_str(r_delta);
                        if let Some(cb) = &on_chunk {
                            cb(crate::llm::StreamChunk::ThoughtDelta(r_delta.to_string()));
                        }
                    }
                }

                // Extract content delta
                if let Some(c_delta) = delta.get("content").and_then(Value::as_str) {
                    if !c_delta.is_empty() {
                        raw_content.push_str(c_delta);
                        if let Some(cb) = &on_chunk {
                            cb(crate::llm::StreamChunk::ContentDelta(c_delta.to_string()));
                        }
                    }
                }

                // Extract tool call deltas
                if let Some(t_deltas) = delta.get("tool_calls").and_then(Value::as_array) {
                    for t_delta in t_deltas {
                        let index = t_delta.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                        let entry = tool_calls_map.entry(index).or_insert_with(|| (None, String::new(), String::new()));
                        if let Some(id) = t_delta.get("id").and_then(Value::as_str) {
                            entry.0 = Some(id.to_string());
                        }
                        if let Some(func) = t_delta.get("function") {
                            if let Some(name) = func.get("name").and_then(Value::as_str) {
                                entry.1.push_str(name);
                            }
                            if let Some(args) = func.get("arguments").and_then(Value::as_str) {
                                entry.2.push_str(args);
                            }
                        }
                    }
                }
            }
        }

        let calls: Vec<Value> = tool_calls_map
            .into_iter()
            .map(|(_idx, (id, name, args))| {
                let parsed_args = serde_json::from_str::<Value>(&args)
                    .unwrap_or(Value::String(args));
                serde_json::json!({
                    "id": id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": parsed_args,
                    }
                })
            })
            .collect();

        if thought.is_empty() {
            if let Some(start) = raw_content.find("<think>") {
                if let Some(end) = raw_content.find("</think>") {
                    thought = raw_content[start + 7..end].trim().to_string();
                    raw_content = format!("{}{}", &raw_content[..start], &raw_content[end + 8..]).trim().to_string();
                }
            }
        }
        Ok(serde_json::json!({
            "content": raw_content,
            "thought": if thought.is_empty() { Value::Null } else { Value::String(thought) },
            "tool_calls": calls,
        }))
    }

    /// The OpenAI-compatible `GET /models` endpoint answers with
    /// `{ "data": [{ "id": ... }] }`; OpenRouter, LM Studio and vLLM mirror it.
    async fn list_models(&self) -> LlmResult<Vec<String>> {
        let settings = self.settings();
        let base_url = Self::api_url(&settings);
        let response = self
            .client
            .get(format!("{}/models", base_url))
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
