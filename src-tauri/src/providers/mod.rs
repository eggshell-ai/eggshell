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
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// How long a fetched model list stays fresh before it is fetched again.
pub const MODEL_CACHE_TTL_SECONDS: u64 = 24 * 60 * 60;

/// The current wall-clock time in whole seconds since the Unix epoch, which is
/// all the model cache needs to age its entries.
pub fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedModels {
    fetched_at: u64,
    models: Vec<String>,
}

/// A small on-disk record of the models each provider last reported, keyed by
/// the provider's registry name. The chat screen asks for models on every load,
/// so the cache keeps that from turning into an upstream request each time; an
/// entry older than [`MODEL_CACHE_TTL_SECONDS`] is treated as absent.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelCache {
    #[serde(default)]
    entries: HashMap<String, CachedModels>,
}

impl ModelCache {
    /// Reads the cache, silently falling back to an empty one when the file is
    /// missing or corrupt: a bad cache should cost a fetch, not an error.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|contents| serde_json::from_str(&contents).ok())
            .unwrap_or_default()
    }

    /// Persists the cache, creating its directory if needed.
    pub fn save(&self, path: &Path) -> LlmResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// The cached models for `key` when the entry is still within its TTL.
    pub fn fresh(&self, key: &str, now: u64) -> Option<Vec<String>> {
        let entry = self.entries.get(key)?;
        (now.saturating_sub(entry.fetched_at) < MODEL_CACHE_TTL_SECONDS).then(|| entry.models.clone())
    }

    /// Records a fresh model list for `key`, stamping it with `now`.
    pub fn store(&mut self, key: &str, models: Vec<String>, now: u64) {
        self.entries.insert(
            key.to_string(),
            CachedModels {
                fetched_at: now,
                models,
            },
        );
    }
}

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
    pub id: String,
    pub key: String,
    pub provider_type: String,
    pub name: String,
    pub title: String,
    pub detail: String,
    pub base_url: Option<String>,
    pub api_key_set: bool,
    pub models: Vec<String>,
    pub reasoning: Option<String>,
}

/// Combines the configured providers with their descriptors and cached models.
/// Each configured provider entry in config.yaml becomes a ProviderSummary.
pub fn provider_summaries(
    configured: &[ProviderConfig],
    cache: &ModelCache,
    now: u64,
) -> Vec<ProviderSummary> {
    let descriptors = registered_providers();
    configured
        .iter()
        .map(|config| {
            let id = config.id();
            let provider_type = config.provider_type();
            let descriptor = descriptors
                .iter()
                .find(|desc| desc.key == provider_type);
            let name = descriptor
                .map(|desc| desc.name.clone())
                .unwrap_or_else(|| provider_type.clone());
            let detail = descriptor
                .map(|desc| desc.detail.clone())
                .unwrap_or_default();
            let title = config.title();

            let mut models = config.models.clone();
            // Cache lookup uses the provider id first, falling back to provider_type.
            // Only adopt cached models if the provider has no models configured.
            if models.is_empty() {
                let cached_models = cache.fresh(&id, now)
                    .or_else(|| cache.fresh(&provider_type, now));
                if let Some(fetched) = cached_models {
                    models = fetched;
                }
            }
            ProviderSummary {
                id: id.clone(),
                key: id,
                provider_type,
                name,
                title,
                detail,
                base_url: config.base_url.clone(),
                api_key_set: !config.api_key.trim().is_empty(),
                models,
                reasoning: config.reasoning.clone(),
            }
        })
        .collect()
}

/// Holds configured provider services and forwards prompts to whichever one
/// is currently active.
pub struct ProviderHub {
    services: RwLock<HashMap<String, Arc<dyn LLMService>>>,
    active: RwLock<Arc<dyn LLMService>>,
}

impl ProviderHub {
    pub fn new(config: Option<&ProviderConfig>) -> Self {
        match config {
            Some(cfg) => Self::from_configs(std::slice::from_ref(cfg)),
            None => Self::from_configs(&[]),
        }
    }

    pub fn from_configs(configs: &[ProviderConfig]) -> Self {
        let mut services = HashMap::new();
        let mut active: Option<Arc<dyn LLMService>> = None;

        for cfg in configs {
            if let Ok(svc) = build_service(cfg) {
                services.insert(cfg.id(), svc.clone());
                services.insert(cfg.provider_type(), svc.clone());
                if active.is_none() {
                    let is_configured = !cfg.api_key.trim().is_empty() || !cfg.models.is_empty();
                    if is_configured {
                        active = Some(svc);
                    }
                }
            }
        }

        let active = active
            .or_else(|| services.values().next().cloned())
            .unwrap_or_else(|| Arc::new(OllamaService::new(OllamaSettings::default())));

        Self {
            services: RwLock::new(services),
            active: RwLock::new(active),
        }
    }

    /// Applies or updates settings for a configured provider and makes it the active one.
    pub fn apply(&self, config: &ProviderConfig) {
        if let Ok(service) = build_service(config) {
            let mut services = self
                .services
                .write()
                .unwrap_or_else(|poison| poison.into_inner());
            services.insert(config.id(), service.clone());
            services.insert(config.provider_type(), service.clone());
            *self
                .active
                .write()
                .unwrap_or_else(|poison| poison.into_inner()) = service;
        }
    }

    fn active(&self) -> Arc<dyn LLMService> {
        self.active
            .read()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }

    /// Looks up the service for a given provider id or provider type.
    pub fn service_for(&self, identifier: &str) -> Option<Arc<dyn LLMService>> {
        self.services
            .read()
            .unwrap_or_else(|poison| poison.into_inner())
            .get(identifier)
            .cloned()
    }

    /// Fetches the models a provider currently offers using its configured credentials.
    pub async fn fetch_models(&self, identifier: &str) -> LlmResult<Vec<String>> {
        match self.service_for(identifier) {
            Some(service) => service.list_models().await,
            None => Ok(Vec::new()),
        }
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

    async fn execute_prompt_with_tools_cancellable(
        &self,
        messages: &[LLMMessage],
        tools: &[Box<dyn Tool>],
        context: Option<&Map<String, Value>>,
        cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
        on_chunk: Option<crate::llm::StreamCallback>,
    ) -> LlmResult<Value> {
        self.active()
            .execute_prompt_with_tools_cancellable(messages, tools, context, cancel, on_chunk)
            .await
    }
}

/// Builds the service that talks to the provider instance using the given
/// configuration.
pub fn build_service(config: &ProviderConfig) -> LlmResult<Arc<dyn LLMService>> {
    let kind = config.provider_type();
    match kind.as_str() {
        "ollama" => Ok(Arc::new(OllamaService::new(OllamaSettings::from(config)))),
        "openai" => Ok(Arc::new(OpenAiService::new(OpenAiSettings::from(config)))),
        other => Err(format!("\"{other}\" is not a supported provider.").into()),
    }
}
