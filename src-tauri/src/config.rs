use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Application configuration loaded from `config.yaml`.
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub ollama: OllamaConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OllamaConfig {
    pub model: String,
    #[serde(rename = "apiKey")]
    pub api_key: String,
}

/// Service responsible for loading and exposing application configuration.
///
/// The service does not load configuration automatically. Call `load` when a
/// consumer is ready to use it.
pub struct ConfigService;

impl ConfigService {
    pub fn load(path: impl AsRef<Path>) -> Result<AppConfig, ConfigError> {
        let path = path.as_ref();
        let contents = fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;

        serde_yaml::from_str(&contents).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })
    }

    pub fn load_default() -> Result<AppConfig, ConfigError> {
        Self::load("config.yaml")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read configuration file {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse configuration file {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },
}
