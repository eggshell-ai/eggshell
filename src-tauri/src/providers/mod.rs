//! The provider registry: every LLM backend Eggshell knows about contributes a
//! descriptor here, and each descriptor knows how to turn its configuration
//! into a running [`LLMService`](crate::llm::LLMService).
//!
//! Adding a provider means adding a module, an entry in
//! [`registered_providers`], and a branch in [`build_service`]; the setup screen
//! and config file pick the rest up automatically.

pub mod ollama;

pub use ollama::{OllamaService, OllamaSettings};

use crate::config::ProviderConfig;
use crate::llm::{LlmResult, LLMService};
use serde::Serialize;
use std::sync::Arc;

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

/// Every provider Eggshell can configure. Ollama for now; each new provider
/// adds itself here and the setup screen lists it without further changes.
pub fn registered_providers() -> Vec<ProviderDescriptor> {
    vec![ProviderDescriptor {
        key: "ollama".to_string(),
        name: "Ollama".to_string(),
        detail: "Cloud and local models from ollama.com".to_string(),
    }]
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

/// Builds the service that talks to the named provider using the given
/// configuration. The provider registry is deliberately closed: an unknown
/// name is a configuration error, not a silently ignored entry.
pub fn build_service(config: &ProviderConfig) -> LlmResult<Arc<dyn LLMService>> {
    match config.name.as_str() {
        "ollama" => Ok(Arc::new(OllamaService::new(OllamaSettings::from(config)))),
        other => Err(format!("\"{other}\" is not a supported provider.").into()),
    }
}
