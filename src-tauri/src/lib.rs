pub mod config;
mod db;
pub mod dependencies;
pub mod llm;
pub mod providers;
pub mod logger;
pub mod migrations;
pub mod progress;
mod setup;
pub mod telemetry;
mod tools;

pub use dependencies::{
    add_managed_tools_to_process_path, managed_bin_dir, managed_composer_present, managed_mysqld,
};

use db::{NewProject, Project, ProjectsRepository, Session, SessionsRepository};
use progress::ProgressLog;
use serde::Serialize;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::net::{SocketAddr, TcpStream};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

type ActiveGenerationMap = Arc<Mutex<HashMap<i64, Arc<AtomicBool>>>>;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
async fn detect_dependencies(
    app: tauri::AppHandle,
    log: tauri::State<'_, ProgressLog>,
) -> Result<dependencies::DependencyStatus, String> {
    let log = log.inner().clone();
    tauri::async_runtime::spawn_blocking(move || dependencies::detect_dependencies_blocking(&app, &log))
        .await
        .map_err(|error| format!("dependency detection failed: {error}"))
}

#[tauri::command]
async fn install_dependency(
    name: String,
    app: tauri::AppHandle,
    log: tauri::State<'_, ProgressLog>,
) -> Result<dependencies::InstallOutcome, String> {
    let log = log.inner().clone();
    let thread_log = log.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || dependencies::install_dependency_blocking(&name, &thread_log))
        .await
        .map_err(|error| format!("installation task failed: {error}"))??;

    if outcome.installed && !outcome.already_present && dependencies::managed_mysqld().is_some() {
        let mut config = config::ConfigService::load_default(&app).unwrap_or_default();
        config.mysql = config::MysqlConfig {
            kind: "managed".to_string(),
            port: 3306,
            user: "root".to_string(),
            pass: String::new(),
            is_mariadb: false,
        };
        if let Err(error) = config::ConfigService::save_default(&app, &config) {
            log.line("warning", format!("could not persist managed mysql configuration: {error}"));
        }
    }

    Ok(outcome)
}

/// Whatever the log has collected so far, so opening the log window shows what
/// happened before it was open: dependency detection, and MySQL's start-up.
#[tauri::command]
fn setup_log_history(log: tauri::State<'_, ProgressLog>) -> Vec<progress::LogLine> {
    log.history()
}

/// The accumulated central-log entries, oldest first, so a report can quote what
/// the application has done since it started. Sensitive entries are excluded
/// unless the caller explicitly asks for them.
#[tauri::command]
fn read_logs(
    log: tauri::State<'_, ProgressLog>,
    include_sensitive: Option<bool>,
) -> Vec<logger::LogEntry> {
    progress::read_logs(log.logger(), include_sensitive.unwrap_or(false))
}

/// What the repository ships in `config.yaml` where a real value belongs, so the
/// setup screen offers an empty field rather than a placeholder to correct.
const CONFIG_PLACEHOLDER: &str = "...";

/// What the setup screen needs to know at launch: whether to appear at all, and
/// which providers exist and how each is configured. The API key is deliberately
/// not sent back to the frontend — only whether one is set.
/// What the setup screen needs to know at launch: whether to appear at all,
#[derive(Debug, Serialize)]
pub struct MysqlSummary {
    pub kind: String,
    pub port: u16,
    pub user: String,
    pub pass_set: bool,
    pub is_mariadb: bool,
}

/// The state returned to the frontend on startup and settings: whether setup is finished,
/// which configured provider instances exist, and which provider types are available.
/// The API key is deliberately not sent back to the frontend — only whether one is set.
#[derive(Debug, Serialize)]
struct SetupState {
    setup_completed: bool,
    providers: Vec<providers::ProviderSummary>,
    available_types: Vec<providers::ProviderDescriptor>,
    mysql: MysqlSummary,
    telemetry_enabled: bool,
    installation_id: String,
}

#[tauri::command]
fn load_setup_state(app: tauri::AppHandle, log: tauri::State<'_, ProgressLog>) -> SetupState {
    // Models fetched earlier are folded in from the cache, so the picker shows
    // them even before a fresh fetch has run.
    let cache = config::ConfigService::model_cache_path(&app)
        .map(|path| providers::ModelCache::load(&path))
        .unwrap_or_default();
    let now = providers::now_seconds();
    let available_types = providers::registered_providers();
    match config::ConfigService::load_default(&app) {
        Ok(config) => {
            let pass_set = !config.mysql.pass.trim().is_empty();
            let mysql = MysqlSummary {
                kind: config.mysql.kind,
                port: config.mysql.port,
                user: config.mysql.user,
                pass_set,
                is_mariadb: config.mysql.is_mariadb,
            };
            SetupState {
                setup_completed: config.setup_completed,
                providers: providers::provider_summaries(&config.providers, &cache, now),
                available_types,
                mysql,
                telemetry_enabled: config.telemetry_enabled,
                installation_id: config.installation_id.unwrap_or_default(),
            }
        }
        // A first launch has no configuration to read yet, which is exactly when
        // setup has to run.
        Err(error) => {
            log.line("info", format!("{error}; treating setup as incomplete"));
            let default_mysql = config::MysqlConfig::default();
            let pass_set = !default_mysql.pass.trim().is_empty();
            let mysql = MysqlSummary {
                kind: default_mysql.kind,
                port: default_mysql.port,
                user: default_mysql.user,
                pass_set,
                is_mariadb: default_mysql.is_mariadb,
            };
            SetupState {
                setup_completed: false,
                providers: providers::provider_summaries(&[], &cache, now),
                available_types,
                mysql,
                telemetry_enabled: false,
                installation_id: String::new(),
            }
        }
    }
}

/// Updates the user's telemetry and error tracking preference in config.yaml
/// and updates the runtime Sentry gate and user scope.
#[tauri::command]
fn save_privacy_settings(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut config = config::ConfigService::load_default(&app).unwrap_or_default();
    config.telemetry_enabled = enabled;
    let installation_id = match &config.installation_id {
        Some(id) if !id.trim().is_empty() => id.clone(),
        _ => {
            let new_id = uuid::Uuid::new_v4().to_string();
            config.installation_id = Some(new_id.clone());
            new_id
        }
    };
    config::ConfigService::save_default(&app, &config).map_err(|error| error.to_string())?;

    telemetry::set_telemetry_enabled(enabled);
    if enabled {
        telemetry::record_startup_event(&installation_id);
    } else {
        telemetry::clear_scope();
    }
    Ok(())
}

/// Fetches the models a provider currently offers, using the credentials from
/// the running hub. Results are cached on disk for 24 hours so the chat screen
/// asking on every load does not become an upstream request every time.
#[tauri::command]
async fn fetch_models(
    provider: String,
    app: tauri::AppHandle,
    log: tauri::State<'_, ProgressLog>,
    hub: tauri::State<'_, Arc<providers::ProviderHub>>,
) -> Result<Vec<String>, String> {
    let cache_path =
        config::ConfigService::model_cache_path(&app).map_err(|error| error.to_string())?;
    let now = providers::now_seconds();
    let mut cache = providers::ModelCache::load(&cache_path);
    // A fresh entry answers immediately and never touches the network.
    if let Some(models) = cache.fresh(&provider, now) {
        return Ok(models);
    }

    // The hub owns the live provider services, so a fetch uses the credentials
    // entered on the setup or settings screen without reading the file again.
    let hub = hub.inner().clone();
    let models = match hub.fetch_models(&provider).await {
        Ok(models) => models,
        Err(error) => {
            let message = error.to_string();
            log.line(
                "warning",
                format!("could not fetch {provider} models: {message}"),
            );
            return Err(message);
        }
    };

    // Providers can list a model more than once; the picker wants a stable set.
    let mut models = models;
    models.sort();
    models.dedup();
    cache.store(&provider, models.clone(), now);
    if let Err(error) = cache.save(&cache_path) {
        log.line(
            "warning",
            format!("could not cache {provider} models: {error}"),
        );
    }
    log.line(
        "info",
        format!("fetched {} {provider} models", models.len()),
    );

    // When the provider has no models of its own yet, the fetched list becomes
    // its models so the agent has something to send and the picker has a default
    // that the backend recognises. A provider with saved models is left alone.
    if !models.is_empty() {
        if let Ok(mut config) = config::ConfigService::load_default(&app) {
            if let Some(entry) = config
                .providers
                .iter_mut()
                .find(|candidate| (candidate.id() == provider || candidate.name == provider) && candidate.models.is_empty())
            {
                entry.models = models.clone();
                let provider_config = entry.clone();
                if let Err(error) = config::ConfigService::save_default(&app, &config) {
                    log.line("warning", format!("could not save fetched {provider} models: {error}"));
                } else {
                    hub.apply(&provider_config);
                    log.line(
                        "info",
                        format!("adopted {provider} models {}", provider_config.models.join(", ")),
                    );
                }
            }
        }
    }
    Ok(models)
}

/// Tests a provider configuration by attempting to fetch its models.
/// Does not persist changes to disk.
#[tauri::command]
async fn test_provider_config(
    provider: Option<String>,
    provider_id: Option<String>,
    provider_type: Option<String>,
    base_url: Option<String>,
    api_key: String,
    app: tauri::AppHandle,
    log: tauri::State<'_, ProgressLog>,
) -> Result<Vec<String>, String> {
    let kind = provider_type
        .as_deref()
        .filter(|t| !t.trim().is_empty())
        .or_else(|| provider.as_deref().filter(|p| !p.trim().is_empty()))
        .unwrap_or("");

    let known = providers::registered_providers()
        .iter()
        .any(|descriptor| descriptor.key == kind);
    if !known {
        return Err(format!("\"{kind}\" is not a supported provider yet."));
    }

    let api_key = api_key.trim().to_string();
    let config = config::ConfigService::load_default(&app).unwrap_or_default();

    let target_id = provider_id
        .as_deref()
        .filter(|id| !id.trim().is_empty())
        .or_else(|| provider.as_deref().filter(|p| !p.trim().is_empty()));

    let existing = target_id.and_then(|id| {
        config
            .providers
            .iter()
            .find(|candidate| candidate.id() == id || candidate.name == id)
    });

    let resolved_api_key = match (api_key.is_empty(), existing) {
        (false, _) => api_key,
        (true, Some(found)) => found.api_key.clone(),
        (true, None) => {
            if kind == "ollama" {
                String::new()
            } else {
                return Err("An API key is required to test the connection.".to_string());
            }
        }
    };

    let final_base_url = base_url
        .filter(|url| !url.trim().is_empty())
        .or_else(|| existing.and_then(|p| p.base_url.clone()));

    let provider_config = config::ProviderConfig {
        id: provider_id.clone(),
        name: kind.to_string(),
        provider_type: Some(kind.to_string()),
        title: None,
        base_url: final_base_url,
        api_key: resolved_api_key,
        models: Vec::new(),
        reasoning: None,
    };

    let service = providers::build_service(&provider_config).map_err(|error| error.to_string())?;
    let mut models = match service.list_models().await {
        Ok(models) => models,
        Err(error) => {
            let message = format!("Connection test failed: {error}");
            log.line(
                "warning",
                format!("test_provider_config failed for {kind}: {error}"),
            );
            return Err(message);
        }
    };

    models.sort();
    models.dedup();

    // Cache the verified models
    if let Ok(cache_path) = config::ConfigService::model_cache_path(&app) {
        let now = providers::now_seconds();
        let mut cache = providers::ModelCache::load(&cache_path);
        if let Some(id) = provider_id.as_deref().filter(|id| !id.trim().is_empty()) {
            cache.store(id, models.clone(), now);
        }
        cache.store(kind, models.clone(), now);
        let _ = cache.save(&cache_path);
    }

    log.line(
        "info",
        format!(
            "test_provider_config succeeded for {kind}, fetched {} models",
            models.len()
        ),
    );

    Ok(models)
}

#[tauri::command]
async fn save_provider_config(
    provider: String,
    provider_id: Option<String>,
    provider_type: Option<String>,
    title: Option<String>,
    base_url: Option<String>,
    models: Vec<String>,
    api_key: String,
    app: tauri::AppHandle,
    log: tauri::State<'_, ProgressLog>,
    hub: tauri::State<'_, Arc<providers::ProviderHub>>,
) -> Result<(), String> {
    // Resolve the provider backend type (e.g. "ollama", "openai")
    let kind = provider_type
        .as_deref()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or(&provider);

    let known = providers::registered_providers()
        .iter()
        .any(|descriptor| descriptor.key == kind);
    if !known {
        return Err(format!("\"{kind}\" is not a supported provider yet."));
    }

    let models = models
        .into_iter()
        .map(|model| model.trim().to_string())
        .filter(|model| !model.is_empty())
        .collect::<Vec<_>>();
    let api_key = api_key.trim().to_string();

    let mut config = config::ConfigService::load_default(&app).unwrap_or_else(|error| {
        log.line("info", format!("{error}; writing a fresh configuration"));
        config::AppConfig::default()
    });

    // Check if updating an existing provider by provider_id or legacy provider key
    let target_id = provider_id
        .as_deref()
        .filter(|id| !id.trim().is_empty())
        .unwrap_or(&provider);

    let existing = config
        .providers
        .iter()
        .position(|candidate| candidate.id() == target_id || candidate.name == target_id);

    let resolved_api_key = match (api_key.is_empty(), existing) {
        (false, _) => api_key,
        (true, Some(index)) => config.providers[index].api_key.clone(),
        (true, None) => {
            if kind == "ollama" {
                String::new()
            } else {
                return Err("An API key is required.".to_string());
            }
        }
    };

    let reasoning = existing.and_then(|index| config.providers[index].reasoning.clone());

    // Generate or preserve an ID
    let final_id = match existing {
        Some(index) => config.providers[index].id(),
        None => {
            if let Some(id) = provider_id.filter(|id| !id.trim().is_empty()) {
                id
            } else {
                let now = providers::now_seconds();
                format!("{}-{}", kind, now % 100_000)
            }
        }
    };

    let final_title = title
        .filter(|t| !t.trim().is_empty())
        .or_else(|| existing.and_then(|idx| config.providers[idx].title.clone()))
        .or_else(|| {
            providers::registered_providers()
                .iter()
                .find(|d| d.key == kind)
                .map(|d| d.name.clone())
        });

    let final_base_url = base_url
        .filter(|url| !url.trim().is_empty())
        .or_else(|| existing.and_then(|idx| config.providers[idx].base_url.clone()));

    let mut provider_config = config::ProviderConfig {
        id: Some(final_id.clone()),
        name: kind.to_string(),
        provider_type: Some(kind.to_string()),
        title: final_title,
        base_url: final_base_url,
        api_key: resolved_api_key,
        models,
        reasoning,
    };

    // Test the provider connection before saving
    let service = providers::build_service(&provider_config).map_err(|error| error.to_string())?;
    let mut fetched_models = match service.list_models().await {
        Ok(models) => models,
        Err(error) => {
            let message = format!("Connection test failed: {error}");
            log.line(
                "warning",
                format!("save_provider_config connection test failed for {kind}: {error}"),
            );
            return Err(message);
        }
    };
    fetched_models.sort();
    fetched_models.dedup();

    // If the provider has no models configured, adopt the models fetched from connection test
    if provider_config.models.is_empty() && !fetched_models.is_empty() {
        provider_config.models = fetched_models.clone();
    }

    if !fetched_models.is_empty() {
        if let Ok(cache_path) = config::ConfigService::model_cache_path(&app) {
            let now = providers::now_seconds();
            let mut cache = providers::ModelCache::load(&cache_path);
            cache.store(&final_id, fetched_models.clone(), now);
            cache.store(kind, fetched_models.clone(), now);
            let _ = cache.save(&cache_path);
        }
    }

    // If the config currently only has an unconfigured default Ollama provider,
    // remove it so the newly configured provider becomes the primary provider.
    if existing.is_none() && config.providers.len() == 1 {
        let first = &config.providers[0];
        if first.id() == "ollama" && first.api_key.trim().is_empty() && first.models.is_empty() {
            config.providers.clear();
        }
    }

    match existing {
        Some(index) => {
            config.providers.remove(index);
            config.providers.insert(0, provider_config.clone());
        }
        None => config.providers.insert(0, provider_config.clone()),
    }
    config.setup_completed = true;
    config::ConfigService::save_default(&app, &config).map_err(|error| {
        let message = error.to_string();
        log.line("error", message.clone());
        message
    })?;

    hub.apply(&provider_config);
    log.line(
        "info",
        format!(
            "saved {} ({}) with models {}",
            provider_config.title(),
            final_id,
            provider_config.models.join(", ")
        ),
    );
    Ok(())
}

/// Removes a provider from configuration by id.
#[tauri::command]
fn delete_provider(
    provider: String,
    app: tauri::AppHandle,
    log: tauri::State<'_, ProgressLog>,
) -> Result<(), String> {
    let mut config = config::ConfigService::load_default(&app).map_err(|error| error.to_string())?;
    let before = config.providers.len();
    config.providers.retain(|candidate| candidate.id() != provider && candidate.name != provider);
    if config.providers.len() == before {
        return Err(format!("{provider} has not been configured yet."));
    }
    config::ConfigService::save_default(&app, &config).map_err(|error| error.to_string())?;
    log.line("info", format!("removed {provider} from configuration"));
    Ok(())
}

/// Switches the model the running agent uses.
#[tauri::command]
fn select_model(
    provider: String,
    model: String,
    reasoning: Option<String>,
    app: tauri::AppHandle,
    log: tauri::State<'_, ProgressLog>,
    hub: tauri::State<'_, Arc<providers::ProviderHub>>,
) -> Result<(), String> {
    let model = model.trim().to_string();
    if model.is_empty() {
        return Err("A model is required.".to_string());
    }

    let mut config = config::ConfigService::load_default(&app).map_err(|error| error.to_string())?;
    let cached = config::ConfigService::model_cache_path(&app)
        .ok()
        .map(|path| providers::ModelCache::load(&path))
        .and_then(|cache| cache.fresh(&provider, providers::now_seconds()))
        .unwrap_or_default();
    let provider_config = config
        .providers
        .iter_mut()
        .find(|candidate| candidate.id() == provider || candidate.name == provider)
        .ok_or_else(|| format!("{provider} has not been configured yet."))?;
    if !provider_config.models.iter().any(|candidate| candidate == &model)
        && !cached.iter().any(|candidate| candidate == &model)
    {
        return Err(format!("\"{model}\" is not a configured {provider} model."));
    }
    // Adopt a fetched model on selection so it survives the cache expiring.
    if !provider_config.models.iter().any(|candidate| candidate == &model) {
        provider_config.models.push(model.clone());
    }
    // Move the selection to the front, so it is also the default on the next launch.
    provider_config.models.retain(|candidate| candidate != &model);
    provider_config.models.insert(0, model.clone());
    if let Some(r) = reasoning {
        provider_config.reasoning = Some(r);
    }
    let target_id = provider_config.id();
    let provider_config = provider_config.clone();

    // Also move this provider to the front of config.providers so it remains default on launch
    if let Some(pos) = config.providers.iter().position(|p| p.id() == target_id) {
        let p = config.providers.remove(pos);
        config.providers.insert(0, p);
    }

    config::ConfigService::save_default(&app, &config).map_err(|error| error.to_string())?;

    hub.apply(&provider_config);
    log.line("info", format!("switched to {} model {model} (reasoning: {:?})", provider_config.title(), provider_config.reasoning));
    Ok(())
}

#[tauri::command]
async fn list_projects(pool: tauri::State<'_, SqlitePool>) -> Result<Vec<Project>, String> {
    ProjectsRepository::list(pool.inner())
        .await
        .map_err(|error| error.to_string())
}

fn bundled_template_root(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|error| format!("Could not locate bundled resources: {error}"))?;

    // Development builds and some bundle formats place resources directly
    // below resource_dir. On Windows, the packaged application may place
    // them below the updater staging directory instead.
    let candidates = [
        resource_dir.join("templates"),
        resource_dir.join("_up_").join("templates"),
    ];

    candidates
        .iter()
        .find(|candidate| candidate.is_dir())
        .cloned()
        .ok_or_else(|| {
            format!(
                "Bundled templates were not found. Checked: {} and {}",
                candidates[0].display(),
                candidates[1].display()
            )
        })
}

#[tauri::command]
async fn create_project(
    project: NewProject,
    pool: tauri::State<'_, SqlitePool>,
    app: tauri::AppHandle,
) -> Result<Project, String> {
    if project.title.trim().is_empty() || project.path.trim().is_empty() {
        return Err("A project title and folder are required.".to_string());
    }

    let template_root = bundled_template_root(&app)?;

    // A log of its own per creation, rather than the managed setup log: the two are
    // read by different windows, and this one's channels are the shells.
    let log = ProgressLog::new(app.clone(), "project-log", "project");

    let (mysql_password, is_mariadb) = config::ConfigService::load_default(&app)
        .map(|config| (config.mysql.pass, config.mysql.is_mariadb))
        .unwrap_or_default();
    ProjectsRepository::create(
        pool.inner(),
        project,
        &template_root,
        &log,
        &mysql_password,
        is_mariadb,
    )
    .await
    .map_err(|error| error.to_string())
}

#[tauri::command]
async fn delete_project(id: i64, pool: tauri::State<'_, SqlitePool>) -> Result<(), String> {
    ProjectsRepository::delete(pool.inner(), id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn start_project(id: i64, pool: tauri::State<'_, SqlitePool>) -> Result<(), String> {
    ProjectsRepository::start(pool.inner(), id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn list_sessions(
    project_id: i64,
    pool: tauri::State<'_, SqlitePool>,
) -> Result<Vec<Session>, String> {
    SessionsRepository::list(pool.inner(), project_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn delete_session(
    project_id: i64,
    id: i64,
    pool: tauri::State<'_, SqlitePool>,
) -> Result<(), String> {
    SessionsRepository::delete(pool.inner(), project_id, id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn send_message(
    project_id: i64,
    session_id: Option<i64>,
    message: String,
    artifacts: Option<Vec<String>>,
    mode: Option<String>,
    app: tauri::AppHandle,
    pool: tauri::State<'_, SqlitePool>,
    agent: tauri::State<'_, llm::AgentService>,
    active_generations: tauri::State<'_, ActiveGenerationMap>,
) -> Result<Session, String> {
    let message = message.trim().to_string();
    if message.is_empty() {
        return Err("A message is required.".to_string());
    }

    if session_id.is_none() && telemetry::is_telemetry_enabled() {
        let config = config::ConfigService::load_default(&app).unwrap_or_default();
        let installation_id = config.installation_id.unwrap_or_default();
        if !installation_id.is_empty() {
            telemetry::record_conversation_started_event(&installation_id);
        }
    }

    // Only the names cross the boundary here: the files themselves are never
    // uploaded to the upstream provider, the agent just learns what was attached.
    let artifacts = artifacts
        .unwrap_or_default()
        .into_iter()
        .filter(|name| !name.trim().is_empty())
        .map(|name| llm::AgentArtifact { name })
        .collect::<Vec<_>>();
    let event_sink = Arc::new({
        let app = app.clone();
        move |payload| {
            let _ = app.emit("agent-event", payload);
        }
    });

    let cancel_flag = Arc::new(AtomicBool::new(false));
    {
        let mut map = active_generations.lock().unwrap_or_else(|p| p.into_inner());
        map.insert(project_id, cancel_flag.clone());
    }

    let result = SessionsRepository::save_exchange(
        pool.inner(),
        project_id,
        session_id,
        message,
        artifacts,
        agent.inner(),
        event_sink,
        mode,
        Some(&app),
        Some(cancel_flag),
    )
    .await;

    {
        let mut map = active_generations.lock().unwrap_or_else(|p| p.into_inner());
        map.remove(&project_id);
    }

    result.map_err(|error| error.to_string())
}

#[tauri::command]
async fn stop_chat(
    project_id: i64,
    active_generations: tauri::State<'_, ActiveGenerationMap>,
) -> Result<(), String> {
    let map = active_generations.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(flag) = map.get(&project_id) {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[derive(Debug, serde::Deserialize)]
pub struct SubmitFeedbackPayload {
    pub rating: String,
    pub reason: Option<String>,
    pub notes: Option<String>,
    #[serde(rename = "attachPromptHistory", default)]
    pub attach_prompt_history: bool,
    #[serde(rename = "promptHistory")]
    pub prompt_history: Option<Vec<telemetry::FeedbackMessagePayload>>,
    pub provider: Option<String>,
    pub model: Option<String>,
}

#[tauri::command]
async fn submit_feedback(
    app: tauri::AppHandle,
    log: tauri::State<'_, ProgressLog>,
    payload: SubmitFeedbackPayload,
) -> Result<(), String> {
    let config = config::ConfigService::load_default(&app).unwrap_or_default();
    let installation_id = config.installation_id.unwrap_or_default();
    let app_version = app.package_info().version.to_string();

    let (prompt_history, diagnostic_logs) = if payload.attach_prompt_history {
        let logs = progress::read_logs(log.logger(), true);
        (payload.prompt_history, Some(logs))
    } else {
        (None, None)
    };

    telemetry::record_feedback_event(
        &installation_id,
        &app_version,
        &payload.rating,
        payload.reason.as_deref(),
        payload.notes.as_deref(),
        payload.provider.as_deref(),
        payload.model.as_deref(),
        prompt_history.as_deref(),
        diagnostic_logs.as_deref(),
    );

    Ok(())
}

#[tauri::command]
fn get_app_version(app: tauri::AppHandle) -> String {
    app.package_info().version.to_string()
}


/// The question a project asks of MySQL too, so it is the one worth asking here:
/// can anything accept a connection on the configured port? A registered service,
/// a server the user started by hand and a daemon left over from an earlier launch
/// all answer yes, and none of them wants a second copy started alongside it.
fn mysql_accepting_connections(port: u16) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&address, Duration::from_millis(400)).is_ok()
}

/// How long to wait for a freshly started daemon before leaving it to finish on
/// its own: MySQL replays the redo log on an unclean shutdown, which is slow, but
/// nobody is kept waiting either way because this runs off the main thread.
const MYSQL_STARTUP_TIMEOUT: Duration = Duration::from_secs(20);

/// Starts the MySQL server Eggshell manages so a project can connect the moment
/// it is created, instead of failing on a database nobody remembered to start.
///
/// Nothing here is fatal: the app has plenty to do without a database, and the
/// setup screen — not a failed launch — is where a missing MySQL belongs. Every
/// dead end is logged and returned from.
fn start_managed_mysql(mysql: &config::MysqlConfig, log: &ProgressLog) {
    if mysql.kind != "managed" {
        log.line(
            "info",
            format!(
                "mysql: configured as \"{}\", so starting the server is left to whoever owns it",
                mysql.kind
            ),
        );
        return;
    }
    if mysql_accepting_connections(mysql.port) {
        log.line(
            "info",
            format!(
                "mysql: already accepting connections on 127.0.0.1:{}",
                mysql.port
            ),
        );
        return;
    }

    let Some(base) = setup::managed_mysql_dir() else {
        log.line("info", "mysql: this platform installs MySQL through a package manager, which starts it as a service");
        return;
    };
    let Some(daemon) = managed_mysqld() else {
        log.line(
            "info",
            format!(
                "mysql: no server has been downloaded to {} yet; run setup to install one",
                base.display()
            ),
        );
        return;
    };

    // `--initialize-insecure` creates the system tables under `data/mysql`, and
    // mysqld refuses to start without them. A half-finished install is the likely
    // cause, so name the fix rather than letting the daemon exit unexplained.
    let data = base.join("data");
    if !data.join("mysql").is_dir() {
        log.line(
            "error",
            format!(
                "mysql: {} has not been initialized; run setup again to rebuild it",
                data.display()
            ),
        );
        return;
    }

    // Keep MySQL's paths in the configuration file created by setup. In particular,
    // --defaults-file must be the first option so mysqld reads basedir/datadir from
    // the managed install's my.ini rather than relying on its working directory.
    let mut command = Command::new(&daemon);
    command
        .arg(format!("--defaults-file={}", base.join("my.ini").display()))
        .arg(format!("--port={}", mysql.port))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // Without this the daemon opens a console window in front of the app on every
    // launch. mysqld keeps writing its own error log inside the data directory.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            log.line(
                "error",
                format!("mysql: could not start {}: {error}", daemon.display()),
            );
            return;
        }
    };
    let pid = child.id();
    log.line(
        "info",
        format!(
            "mysql: starting {} on port {} (PID {pid})",
            daemon.display(),
            mysql.port
        ),
    );

    // mysqld reports a locked data directory or a taken port by exiting, so wait
    // for the port before calling this a success — a silent failure here would
    // surface much later as a project that cannot reach its database.
    let deadline = Instant::now() + MYSQL_STARTUP_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                log.line(
                    "error",
                    format!(
                        "mysql: PID {pid} exited with {status}; the error log in {} says why",
                        data.display()
                    ),
                );
                return;
            }
            Ok(None) => {}
            Err(error) => log.line(
                "info",
                format!("mysql: could not check on PID {pid}: {error}"),
            ),
        }
        if mysql_accepting_connections(mysql.port) {
            log.line(
                "info",
                format!("mysql: accepting connections on 127.0.0.1:{}", mysql.port),
            );
            return;
        }
        if Instant::now() >= deadline {
            log.line("info", format!("mysql: PID {pid} is running but has not opened port {} yet; leaving it to finish starting", mysql.port));
            return;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    add_managed_tools_to_process_path();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Auto-update plumbing. Registered inside setup so the updater and
            // process plugins are available on desktop builds only.
            #[cfg(desktop)]
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())?;

            #[cfg(desktop)]
            app.handle().plugin(tauri_plugin_process::init())?;

            let pool = tauri::async_runtime::block_on(db::initialize(app.handle()))
                .expect("failed to initialize SQLite database");
            app.manage(pool);
            // Built before anything else it could record, and managed so every
            // setup command writes to the same log the setup screen reads.
            let log = ProgressLog::new(app.handle().clone(), "setup-log", "setup");
            app.manage(log.clone());

            // Bring an older configuration file up to the current shape before
            // it is read. Failures are logged rather than fatal: a migration
            // that cannot run leaves the file untouched, and the typed load
            // below still has the previous behaviour to fall back on.
            if let Err(error) = migrations::run(app, &log) {
                log.line("error", format!("config migration failed: {error}"));
            }

            // A missing or unconfigured file is what a first launch looks like.
            let mut config = config::ConfigService::load_default(app).unwrap_or_else(|error| {
                log.line(
                    "info",
                    format!("{error}; starting with empty provider settings"),
                );
                config::AppConfig::default()
            });

            // Ensure an installation ID is generated and persisted in config.yaml
            let mut config_dirty = false;
            let installation_id = match &config.installation_id {
                Some(id) if !id.trim().is_empty() => id.clone(),
                _ => {
                    let new_id = uuid::Uuid::new_v4().to_string();
                    config.installation_id = Some(new_id.clone());
                    config_dirty = true;
                    new_id
                }
            };
            if config_dirty {
                if let Err(e) = config::ConfigService::save_default(app, &config) {
                    log.line("error", format!("failed to persist installation ID: {e}"));
                }
            }

            // If user opted in to telemetry, initialize runtime gate and record startup event
            if config.telemetry_enabled {
                telemetry::set_telemetry_enabled(true);
                telemetry::record_startup_event(&installation_id);
            } else {
                telemetry::set_telemetry_enabled(false);
            }

            // The first configured provider is the one the agent talks to; the
            // chat screen can switch models within it. The hub holds every
            // concrete provider service and forwards prompts to the active one,
            // so the setup screen can switch providers while Eggshell runs.
            let hub = Arc::new(providers::ProviderHub::from_configs(&config.providers));
            // The agent shares the central logger so its prompts and tool calls
            // land in the same log the report menu reads from.
            let agent =
                llm::AgentService::new(hub.clone()).with_logger(log.logger().clone());
            app.manage(agent);
            app.manage(hub);
            let active_generations: ActiveGenerationMap = Arc::new(Mutex::new(HashMap::new()));
            app.manage(active_generations);

            // Probing the port and waiting for the daemon both block, and the
            // window should not wait on a database it does not use itself.
            let mysql = config.mysql;
            tauri::async_runtime::spawn_blocking(move || start_managed_mysql(&mysql, &log));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            detect_dependencies,
            install_dependency,
            setup_log_history,
            read_logs,
            load_setup_state,
            save_privacy_settings,
            config::save_mysql_config,
            config::save_mysql_settings,
            save_provider_config,
            test_provider_config,
            delete_provider,
            select_model,
            fetch_models,
            list_projects,
            create_project,
            delete_project,
            start_project,
            list_sessions,
            delete_session,
            send_message,
            stop_chat,
            submit_feedback,
            get_app_version
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
