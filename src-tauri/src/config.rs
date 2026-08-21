use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Application configuration loaded from `config.yaml`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub ollama: OllamaConfig,
    /// Written once the setup screen finishes, so later launches skip it. Older
    /// configuration files predate the flag, hence the default.
    #[serde(rename = "setupCompleted", default)]
    pub setup_completed: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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

/// Resolved against the working directory, which is `src-tauri` under
/// `tauri dev`. Loading and saving share it so the two can never drift apart.
const DEFAULT_CONFIG_PATH: &str = "config.yaml";

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
        Self::load(DEFAULT_CONFIG_PATH)
    }

    /// Rewrites the whole file from `config`, so anything the struct does not
    /// describe — comments included — is not preserved.
    pub fn save(path: impl AsRef<Path>, config: &AppConfig) -> Result<(), ConfigError> {
        let path = path.as_ref();
        let contents =
            serde_yaml::to_string(config).map_err(|source| ConfigError::Serialize { source })?;

        fs::write(path, contents).map_err(|source| ConfigError::Write {
            path: path.to_path_buf(),
            source,
        })
    }

    pub fn save_default(config: &AppConfig) -> Result<(), ConfigError> {
        Self::save(DEFAULT_CONFIG_PATH, config)
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
    #[error("failed to write configuration file {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to serialize configuration: {source}")]
    Serialize {
        #[source]
        source: serde_yaml::Error,
    },
}
