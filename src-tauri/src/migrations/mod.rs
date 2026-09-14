//! Version-gated migrations for the on-disk configuration file.
//!
//! Eggshell's `config.yaml` changes shape between releases. A migration
//! rewrites a file written by an older release into the current shape, and is
//! skipped once it has run. Each migration is tagged with the first release
//! version whose format is the new one, so the runner can decide what to apply:
//!
//! ```text
//! stored < migration.version() <= running
//! ```
//!
//! The stored side is the `version` key written into `config.yaml`; a file
//! without one is read as `0.0.0`, which is older than any release and therefore
//! due every migration it predates. Migrations work on the raw [`Value`] of the
//! document rather than [`AppConfig`](crate::config::AppConfig), because the
//! shapes they replace are not representable by the current struct.

mod ollama_providers;

use crate::progress::ProgressLog;
pub use ollama_providers::OllamaProvidersMigration;
use semver::Version;
use serde_yaml::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tauri::{Manager, Runtime};

/// Everything that can stop a migration: reading the file, understanding it,
/// transforming it, or writing it back. A caller decides whether that is fatal;
/// the startup runner logs and carries on.
#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("failed to resolve the application configuration directory: {message}")]
    Path { message: String },
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
    #[error("the running version could not be read: {message}")]
    Version { message: String },
    #[error("failed to apply a configuration migration: {message}")]
    Apply { message: String },
    #[error("failed to back up configuration file {path}: {source}")]
    Backup {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to serialize the migrated configuration: {source}")]
    Serialize {
        #[source]
        source: serde_yaml::Error,
    },
    #[error("failed to write configuration file {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// One step in the configuration's history. Implementations are cheap, stateless
/// and idempotent: applying a migration twice must leave the same document.
pub trait ConfigMigration {
    /// The first release whose configuration format is the new one. A file
    /// written below this version is due the migration; one at or above it has
    /// already been through it.
    fn version(&self) -> Version;

    /// One line describing the change, logged when the migration runs.
    fn description(&self) -> &str;

    /// Rewrites the raw document in place. Must leave a document that is already
    /// in the new shape untouched.
    fn apply(&self, document: &mut Value) -> Result<(), MigrationError>;
}

/// Every migration Eggshell ships. Order here does not matter — the runner sorts
/// by version — but keeping them grouped by release makes the list readable.
pub fn registered() -> Vec<Box<dyn ConfigMigration>> {
    vec![Box::new(OllamaProvidersMigration)]
}

/// The migrations that are due for a file last written at `stored`, sorted
/// oldest first so a chain of them rewrites in sequence.
fn pending_migrations(stored: &Version, current: &Version) -> Vec<Box<dyn ConfigMigration>> {
    let mut pending: Vec<Box<dyn ConfigMigration>> = registered()
        .into_iter()
        .filter(|migration| {
            // The running app must be at or past the migration's release...
            &migration.version() <= current
                // ...and the file must predate it. A downgrade satisfies neither.
                && stored < &migration.version()
        })
        .collect();
    pending.sort_by_key(|migration| migration.version());
    pending
}

/// The version recorded in the file, read as `0.0.0` when it is absent or not a
/// parseable version. Both cases mean "written before versions were recorded",
/// which is the oldest possible file.
fn stored_version(document: &Value) -> Version {
    document
        .get("version")
        .and_then(Value::as_str)
        .and_then(|raw| Version::parse(raw.trim()).ok())
        .unwrap_or_else(|| Version::new(0, 0, 0))
}

/// The running release, straight from the package manifest.
fn running_version() -> Result<Version, MigrationError> {
    Version::parse(env!("CARGO_PKG_VERSION")).map_err(|error| MigrationError::Version {
        message: error.to_string(),
    })
}

/// A copy of the file beside the original, labelled with the version that wrote
/// it, so a bad rewrite can be undone by hand.
fn backup_path(path: &Path, stored: &Version) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(format!(".bak-{stored}"));
    PathBuf::from(name)
}

/// Migrates `config.yaml` in place when the file is older than the running
/// release and at least one migration is due.
///
/// Nothing is written unless every due migration succeeds, so a failure leaves
/// the user's configuration exactly as it was.
pub fn run<R: Runtime, M: Manager<R>>(
    manager: &M,
    log: &ProgressLog,
) -> Result<(), MigrationError> {
    let path = crate::config::ConfigService::default_path(manager).map_err(|error| {
        MigrationError::Path {
            message: error.to_string(),
        }
    })?;

    // A first launch has no file to migrate; the typed load handles the empty
    // configuration that follows.
    if !path.exists() {
        return Ok(());
    }

    let contents = fs::read_to_string(&path).map_err(|source| MigrationError::Read {
        path: path.clone(),
        source,
    })?;
    let mut document: Value =
        serde_yaml::from_str(&contents).map_err(|source| MigrationError::Parse {
            path: path.clone(),
            source,
        })?;

    let stored = stored_version(&document);
    let current = running_version()?;
    let pending = pending_migrations(&stored, &current);
    if pending.is_empty() {
        return Ok(());
    }

    let backup = backup_path(&path, &stored);
    fs::copy(&path, &backup).map_err(|source| MigrationError::Backup {
        path: backup.clone(),
        source,
    })?;

    for migration in &pending {
        log.line(
            "info",
            format!(
                "config migration {stored} -> {current}: {}",
                migration.description()
            ),
        );
        migration.apply(&mut document)?;
    }

    // Stamp the running version so none of these run again. This is also what
    // makes the next launch a no-op.
    if let Some(mapping) = document.as_mapping_mut() {
        mapping.insert(
            Value::String("version".to_string()),
            Value::String(current.to_string()),
        );
    }

    let contents =
        serde_yaml::to_string(&document).map_err(|source| MigrationError::Serialize { source })?;
    fs::write(&path, contents).map_err(|source| MigrationError::Write {
        path: path.clone(),
        source,
    })?;

    log.line(
        "info",
        format!("config migrated to {current}; previous file kept at {}", backup.display()),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(raw: &str) -> Version {
        Version::parse(raw).expect("test versions are valid semver")
    }

    #[test]
    fn absent_version_reads_as_the_oldest() {
        let document: Value = serde_yaml::from_str("providers: []").unwrap();
        assert_eq!(stored_version(&document), Version::new(0, 0, 0));
    }

    #[test]
    fn unparseable_version_reads_as_the_oldest() {
        let document: Value = serde_yaml::from_str("version: not-a-version").unwrap();
        assert_eq!(stored_version(&document), Version::new(0, 0, 0));
    }

    #[test]
    fn recorded_version_is_read_back() {
        let document: Value = serde_yaml::from_str("version: 1.0.4").unwrap();
        assert_eq!(stored_version(&document), version("1.0.4"));
    }

    #[test]
    fn migration_runs_for_an_older_file() {
        let pending = pending_migrations(&version("1.0.3"), &version("1.0.4"));
        assert_eq!(pending.len(), 1);
    }

    #[test]
    fn migration_runs_for_a_file_without_a_version() {
        let pending = pending_migrations(&Version::new(0, 0, 0), &version("1.0.4"));
        assert_eq!(pending.len(), 1);
    }

    #[test]
    fn migration_is_skipped_once_it_has_run() {
        let pending = pending_migrations(&version("1.0.4"), &version("1.0.4"));
        assert!(pending.is_empty());
    }

    #[test]
    fn migration_is_skipped_when_the_app_is_older() {
        // A downgrade must not rewrite a newer file.
        let pending = pending_migrations(&Version::new(0, 0, 0), &version("1.0.3"));
        assert!(pending.is_empty());
    }
}
