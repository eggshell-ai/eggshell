use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tauri::{Manager, Runtime};

/// Application configuration loaded from `config.yaml`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub ollama: OllamaConfig,
    #[serde(default)]
    pub mysql: MysqlConfig,
    /// Written once the setup screen finishes, so later launches skip it. Older
    /// configuration files predate the flag, hence the default.
    #[serde(rename = "setupCompleted", default)]
    pub setup_completed: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MysqlConfig { #[serde(rename = "type", default = "default_mysql_type")] pub kind: String, #[serde(default = "default_mysql_port")] pub port: u16, #[serde(default = "default_mysql_user")] pub user: String, #[serde(default)] pub pass: String }
fn default_mysql_type() -> String { "managed".to_string() }
fn default_mysql_port() -> u16 { 336 }
fn default_mysql_user() -> String { "root".to_string() }

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

impl ConfigService {
    /// Returns the per-user, platform-specific configuration path and creates
    /// its parent directory when necessary.
    pub fn default_path<R: Runtime, M: Manager<R>>(manager: &M) -> Result<PathBuf, ConfigError> {
        let directory = manager.path().app_config_dir().map_err(|error| ConfigError::Path {
            message: error.to_string(),
        })?;
        fs::create_dir_all(&directory).map_err(|source| ConfigError::Directory {
            path: directory.clone(),
            source,
        })?;
        Ok(directory.join("config.yaml"))
    }

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

    pub fn load_default<R: Runtime, M: Manager<R>>(manager: &M) -> Result<AppConfig, ConfigError> {
        Self::load(Self::default_path(manager)?)
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

    pub fn save_default<R: Runtime, M: Manager<R>>(manager: &M, config: &AppConfig) -> Result<(), ConfigError> {
        Self::save(Self::default_path(manager)?, config)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to resolve the application configuration directory: {message}")]
    Path { message: String },
    #[error("failed to create configuration directory {path}: {source}")]
    Directory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
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
