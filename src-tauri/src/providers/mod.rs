//! The provider registry: every LLM backend Eggshell knows about contributes a
//! descriptor here, and each descriptor knows how to turn its configuration
//! into a running [`LLMService`](crate::llm::LLMService).
//!
//! Adding a provider means adding a module, an entry in
//! [`registered_providers`], and a branch in [`build_service`]; the setup screen
//! and config file pick the rest up automatically.

pub mod ollama;
pub mod openai;

pub use ollama::{OllamaService, OllamaSettings};
pub use openai::{OpenAiService, OpenAiSettings};

use crate::config::ProviderConfig;
use crate::llm::{LlmResult, LLMMessage, LLMService, Tool};
use async_trait::async_trait;
use serde::Serialize;
use serde_json::{Map, Value};
use std::sync::{Arc, RwLock};

/// Static description of a provider, independent of any user configuration.
/// This is what the setup screen lists before anything has been configured.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderDescriptor {
    /// The identifier used in config.yaml and by the frontend.
    pub key: String,
    /// Human-readable name shown in the setup screen.
    pub name: String,
    /// One-line description shown under the name.
    pub detail: String,
}

/// Every provider Eggshell can configure. Each new provider adds itself here
/// and the setup screen lists it without further changes.
pub fn registered_providers() -> Vec<ProviderDescriptor> {
    vec![
        ProviderDescriptor {
            key: "ollama".to_string(),
            name: "Ollama".to_string(),
            detail: "Cloud and local models from ollama.com".to_string(),
        },
        ProviderDescriptor {
            key: "openai".to_string(),
            name: "OpenAI compatible".to_string(),
            detail: "Any OpenAI-compatible chat completions API (OpenAI, OpenRouter, LM Studio, vLLM)".to_string(),
        },
    ]
}

/// A configured provider merged with its static description, as the frontend
/// receives it: what it is called, and which models the user has added.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderSummary {
    pub key: String,
    pub name: String,
    pub detail: String,
    pub api_key_set: bool,
    pub models: Vec<String>,
}

/// Combines the registry with the configuration, so every registered provider
/// appears (configured or not) and each carries its model list.
pub fn provider_summaries(configured: &[ProviderConfig]) -> Vec<ProviderSummary> {
    registered_providers()
        .into_iter()
        .map(|descriptor| {
            let config = configured
                .iter()
                .find(|provider| provider.name == descriptor.key);
            ProviderSummary {
                key: descriptor.key,
                name: descriptor.name,
                detail: descriptor.detail,
                api_key_set: config.is_some_and(|provider| !provider.api_key.is_empty()),
                models: config.map(|provider| provider.models.clone()).unwrap_or_default(),
            }
        })
        .collect()
}

/// Holds every concrete provider service and forwards prompts to whichever one
/// is currently active. The setup screen re-points it at runtime without the
/// agent (which owns it as a plain `Arc<dyn LLMService>`) being rebuilt.
pub struct ProviderHub {
    ollama: Arc<OllamaService>,
    openai: Arc<OpenAiService>,
    active: RwLock<Arc<dyn LLMService>>,
}

impl ProviderHub {
    /// Starts with the provider named in the configuration, or an empty
    /// Ollama service (whose per-request errors say it is unconfigured).
    pub fn new(config: Option<&ProviderConfig>) -> Self {
        let ollama = Arc::new(OllamaService::new(
            config
                .filter(|provider| provider.name == "ollama")
                .map(OllamaSettings::from)
                .unwrap_or_default(),
        ));
        let openai = Arc::new(OpenAiService::new(
            config
                .filter(|provider| provider.name == "openai")
                .map(OpenAiSettings::from)
                .unwrap_or_default(),
        ));
        let active: Arc<dyn LLMService> = match config.map(|provider| provider.name.as_str()) {
            Some("openai") => openai.clone(),
            _ => ollama.clone(),
        };
        Self {
            ollama,
            openai,
            active: RwLock::new(active),
        }
    }

    /// Applies new settings and makes that provider the active one.
    pub fn apply(&self, config: &ProviderConfig) {
        let active: Arc<dyn LLMService> = match config.name.as_str() {
            "openai" => {
                self.openai.apply(OpenAiSettings::from(config));
                self.openai.clone()
            }
            // The registry is closed; anything else keeps behaving like the
            // Ollama-only era by treating ollama as the default.
            _ => {
                self.ollama.apply(OllamaSettings::from(config));
                self.ollama.clone()
            }
        };
        *self
            .active
            .write()
            .unwrap_or_else(|poison| poison.into_inner()) = active;
    }

    fn active(&self) -> Arc<dyn LLMService> {
        self.active
            .read()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }
}

#[async_trait]
impl LLMService for ProviderHub {
    async fn execute_prompt(
        &self,
        prompt: &str,
        context: Option<&Map<String, Value>>,
    ) -> LlmResult<String> {
        self.active().execute_prompt(prompt, context).await
    }

    async fn execute_prompt_with_tools(
        &self,
        messages: &[LLMMessage],
        tools: &[Box<dyn Tool>],
        context: Option<&Map<String, Value>>,
    ) -> LlmResult<Value> {
        self.active()
            .execute_prompt_with_tools(messages, tools, context)
            .await
    }
}

/// Builds the service that talks to the named provider using the given
/// configuration. The provider registry is deliberately closed: an unknown
/// name is a configuration error, not a silently ignored entry.
pub fn build_service(config: &ProviderConfig) -> LlmResult<Arc<dyn LLMService>> {
    match config.name.as_str() {
        "ollama" => Ok(Arc::new(OllamaService::new(OllamaSettings::from(config)))),
        "openai" => Ok(Arc::new(OpenAiService::new(OpenAiSettings::from(config)))),
        other => Err(format!("\"{other}\" is not a supported provider.").into()),
    }
}
