//! Moves the single legacy `ollama` block into the `providers` list.
//!
//! Up to 1.0.3 the configuration named one Ollama endpoint at the top level:
//!
//! ```yaml
//! ollama:
//!   model: gemma4:31b-cloud
//!   apiKey: ...
//! ```
//!
//! 1.0.4 lists providers instead, each with the models it offers:
//!
//! ```yaml
//! providers:
//!   - name: ollama
//!     apiKey: ...
//!     models:
//!       - gemma4:31b-cloud
//! ```
//!
//! Only Ollama is produced: the OpenAI provider did not exist in 1.0.3, so no
//! 1.0.3 file could have configured it. The current struct has no `ollama` field
//! and serde ignores unknown keys, so without this migration a legacy file would
//! parse "successfully" while silently dropping the user's key and model.

use super::{ConfigMigration, MigrationError};
use semver::Version;
use serde_yaml::{Mapping, Value};

/// The release that first wrote `providers`. Files below it carry the legacy
/// `ollama` block.
const MIGRATION_VERSION: &str = "1.0.4";

pub struct OllamaProvidersMigration;

impl OllamaProvidersMigration {
    fn key(name: &str) -> Value {
        Value::String(name.to_string())
    }

    /// Reads a string field from a mapping, treating a non-string or missing
    /// value as absent.
    fn string_field(mapping: &Mapping, name: &str) -> Option<String> {
        mapping.get(Self::key(name)).and_then(Value::as_str).map(str::to_string)
    }

    /// The `providers` sequence, or an empty one when the file has none yet. A
    /// `providers` key of any other shape is a corrupt file worth stopping for,
    /// rather than overwriting user data.
    fn providers(mapping: &mut Mapping) -> Result<&mut Vec<Value>, MigrationError> {
        let key = Self::key("providers");
        match mapping.get(&key) {
            // Missing, or present but empty, both start a fresh list.
            None | Some(Value::Null) => {
                mapping.insert(key.clone(), Value::Sequence(Vec::new()));
            }
            Some(Value::Sequence(_)) => {}
            Some(_) => {
                return Err(MigrationError::Apply {
                    message: "providers is present but is not a list".to_string(),
                })
            }
        }
        match mapping.get_mut(&key) {
            Some(Value::Sequence(entries)) => Ok(entries),
            _ => unreachable!("the providers entry was just normalized to a list"),
        }
    }

    /// Whether a `providers` entry already names Ollama.
    fn names_ollama(entry: &Value) -> bool {
        entry
            .as_mapping()
            .and_then(|mapping| mapping.get(Self::key("name")))
            .and_then(Value::as_str)
            == Some("ollama")
    }
}

impl ConfigMigration for OllamaProvidersMigration {
    fn version(&self) -> Version {
        Version::parse(MIGRATION_VERSION).expect("the migration version is valid semver")
    }

    fn description(&self) -> &str {
        "moved the ollama block into the providers list"
    }

    fn apply(&self, document: &mut Value) -> Result<(), MigrationError> {
        let mapping = document.as_mapping_mut().ok_or_else(|| MigrationError::Apply {
            message: "the configuration is not a YAML mapping".to_string(),
        })?;

        let ollama_key = Self::key("ollama");

        // Lift the legacy values out before touching the mapping, so the borrow
        // ends and the entry can be built from owned data.
        let (api_key, model) = match mapping.get(&ollama_key) {
            Some(legacy) => {
                let block = legacy.as_mapping().ok_or_else(|| MigrationError::Apply {
                    message: "the ollama block is not a mapping".to_string(),
                })?;
                (
                    Self::string_field(block, "apiKey").unwrap_or_default(),
                    Self::string_field(block, "model").filter(|model| !model.is_empty()),
                )
            }
            // Already migrated, or written by a release that never had the block.
            None => return Ok(()),
        };

        let mut entry = Mapping::new();
        entry.insert(Self::key("name"), Value::String("ollama".to_string()));
        entry.insert(Self::key("apiKey"), Value::String(api_key));
        entry.insert(
            Self::key("models"),
            Value::Sequence(model.into_iter().map(Value::String).collect()),
        );

        let providers = Self::providers(mapping)?;
        // A file may already carry an ollama entry if it was hand-edited; leave
        // that one alone rather than adding a duplicate.
        if !providers.iter().any(Self::names_ollama) {
            providers.insert(0, Value::Mapping(entry));
        }

        // The legacy block has been represented; dropping it is what makes a
        // second run a no-op.
        mapping.remove(&ollama_key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(raw: &str) -> Value {
        let mut document: Value = serde_yaml::from_str(raw).expect("test YAML parses");
        OllamaProvidersMigration
            .apply(&mut document)
            .expect("the migration succeeds");
        document
    }

    fn providers(document: &Value) -> Vec<Value> {
        document
            .get("providers")
            .and_then(Value::as_sequence)
            .cloned()
            .unwrap_or_default()
    }

    #[test]
    fn legacy_block_becomes_an_ollama_provider() {
        let document = apply("ollama:\n  model: gemma4:31b-cloud\n  apiKey: secret\n");
        let providers = providers(&document);
        assert_eq!(providers.len(), 1);
        let entry = providers[0].as_mapping().unwrap();
        assert_eq!(
            entry.get("name").and_then(Value::as_str),
            Some("ollama")
        );
        assert_eq!(
            entry.get("apiKey").and_then(Value::as_str),
            Some("secret")
        );
        assert_eq!(
            entry.get("models"),
            Some(&Value::Sequence(vec![Value::String(
                "gemma4:31b-cloud".to_string()
            )]))
        );
        assert!(!document.as_mapping().unwrap().contains_key("ollama"));
    }

    #[test]
    fn missing_model_leaves_an_empty_model_list() {
        let document = apply("ollama:\n  apiKey: secret\n");
        let entry = &providers(&document)[0];
        assert_eq!(
            entry.as_mapping().unwrap().get("models"),
            Some(&Value::Sequence(Vec::new()))
        );
    }

    #[test]
    fn missing_api_key_becomes_an_empty_string() {
        let document = apply("ollama:\n  model: gemma4:31b-cloud\n");
        let entry = &providers(&document)[0];
        assert_eq!(
            entry.as_mapping().unwrap().get("apiKey"),
            Some(&Value::String(String::new()))
        );
    }

    #[test]
    fn existing_providers_are_preserved() {
        let document = apply(
            "ollama:\n  model: gemma4:31b-cloud\n  apiKey: secret\n\
             providers:\n  - name: something-else\n",
        );
        let providers = providers(&document);
        assert_eq!(providers.len(), 2);
        // The migrated entry is first, the pre-existing one keeps its place.
        assert_eq!(
            providers[0].as_mapping().unwrap().get("name").and_then(Value::as_str),
            Some("ollama")
        );
        assert_eq!(
            providers[1].as_mapping().unwrap().get("name").and_then(Value::as_str),
            Some("something-else")
        );
    }

    #[test]
    fn an_existing_ollama_entry_is_not_duplicated() {
        let document = apply(
            "ollama:\n  model: gemma4:31b-cloud\n  apiKey: secret\n\
             providers:\n  - name: ollama\n    apiKey: kept\n    models: []\n",
        );
        let providers = providers(&document);
        assert_eq!(providers.len(), 1);
        assert_eq!(
            providers[0].as_mapping().unwrap().get("apiKey").and_then(Value::as_str),
            Some("kept")
        );
    }

    #[test]
    fn other_settings_survive_the_rewrite() {
        let document = apply(
            "version: 1.0.3\nollama:\n  model: gemma4:31b-cloud\n  apiKey: secret\n\
             mysql:\n  type: system\n  port: 3306\n  user: root\n  pass: hunter2\n\
             setupCompleted: true\n",
        );
        assert_eq!(
            document.get("mysql").and_then(|mysql| mysql.get("pass")).and_then(Value::as_str),
            Some("hunter2")
        );
        assert_eq!(
            document.get("setupCompleted").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(document.get("version").and_then(Value::as_str), Some("1.0.3"));
    }

    #[test]
    fn applying_twice_changes_nothing_more() {
        let mut document: Value =
            serde_yaml::from_str("ollama:\n  model: gemma4:31b-cloud\n  apiKey: secret\n")
                .unwrap();
        OllamaProvidersMigration.apply(&mut document).unwrap();
        let once = document.clone();
        OllamaProvidersMigration.apply(&mut document).unwrap();
        assert_eq!(document, once);
    }

    #[test]
    fn a_current_document_is_left_alone() {
        let raw = "providers:\n  - name: ollama\n    apiKey: secret\n    models:\n      - m\n";
        let document = apply(raw);
        assert_eq!(
            document,
            serde_yaml::from_str::<Value>(raw).unwrap()
        );
    }

    #[test]
    fn a_malformed_providers_key_is_reported() {
        let mut document: Value = serde_yaml::from_str(
            "ollama:\n  model: gemma4:31b-cloud\nproviders: not-a-list\n",
        )
        .unwrap();
        assert!(OllamaProvidersMigration.apply(&mut document).is_err());
    }
}
