use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{sqlite::SqliteConnectOptions, FromRow, SqlitePool};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};

use crate::llm::{self, AgentArtifact, AgentOptions, AgentService, App, LLMMessage, LLMMessageRole};
use crate::progress::ProgressLog;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Admin credentials seeded into every project by `app:init`
/// (`templates/backend/src/Command/InitCommand.php`). Mirrored into the local
/// `users` table when a project is created.
pub const ADMIN_EMAIL: &str = "admin@dummy-project.com";
pub const ADMIN_PASSWORD: &str = "12345678";

/// Process-wide handle to the SQLite pool.
///
/// Tauri commands receive the pool through managed state, but agent tools are
/// constructed without it. Mirroring [`crate::logger::Logger::global`], this
/// lets any code that runs after startup reach the database.
static POOL: std::sync::OnceLock<SqlitePool> = std::sync::OnceLock::new();

/// Stores the pool for process-wide access. Called once from [`initialize`].
pub fn set_pool(pool: SqlitePool) {
    let _ = POOL.set(pool);
}

/// Returns the process-wide pool, if [`initialize`] has run.
pub fn pool() -> Option<&'static SqlitePool> {
    POOL.get()
}

#[derive(Debug, Serialize, FromRow)]
pub struct Project {
    pub id: i64,
    pub title: String,
    pub slug: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct NewProject {
    pub title: String,
    pub slug: Option<String>,
    pub path: String,
}

#[derive(Debug, Serialize, FromRow)]
pub struct Session {
    pub id: i64,
    pub title: String,
    pub conversation_history: String,
    #[serde(default)]
    pub token_usage: i64,
    pub compacted_summary: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// A sign-in account mirrored from the project's Symfony backend.
///
/// The authoritative record lives in the project's MySQL `user` table; this is
/// the local copy the desktop UI lists (see `0004_create_users.sql`). The
/// password is stored in plain text so it can be shown in the chat sidebar.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    #[serde(default)]
    pub id: i64,
    pub project_id: i64,
    pub email: String,
    #[serde(default = "default_user_role")]
    pub role: String,
    #[serde(default)]
    pub password: String,
}

fn default_user_role() -> String {
    "ROLE_USER".to_string()
}

/// One task in a Max mode build.
///
/// Max mode first breaks a requirement into a list of business-oriented tasks
/// (a screen, CRUD, or report each) and then runs one sequential agent per
/// task. These rows persist that list so a long build can be resumed after a
/// restart or interruption. See `0005_create_max_tasks.sql`.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MaxTask {
    #[serde(default)]
    pub id: i64,
    pub project_id: i64,
    pub session_id: i64,
    pub task_index: i64,
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// One of: `pending`, `running`, `completed`, `failed`, `skipped`.
    #[serde(default = "default_task_status")]
    pub status: String,
    #[serde(default)]
    pub result_summary: Option<String>,
}

fn default_task_status() -> String {
    "pending".to_string()
}

/// Data-access layer for the `projects` table.
pub struct ProjectsRepository;

impl ProjectsRepository {
    pub async fn list(pool: &SqlitePool) -> Result<Vec<Project>, sqlx::Error> {
        sqlx::query_as::<_, Project>(
            "SELECT id, title, slug, path FROM projects ORDER BY title ASC",
        )
        .fetch_all(pool)
        .await
    }

    pub async fn create(
        pool: &SqlitePool,
        project: NewProject,
        template_root: &Path,
        log: &ProgressLog,
        mysql_password: &str,
        is_mariadb: bool,
    ) -> Result<Project, Box<dyn std::error::Error + Send + Sync>> {
        let NewProject { title, slug, path } = project;
        let title = title.trim().to_string();
        let path = path.trim().to_string();
        let slug = match slug.filter(|slug| !slug.trim().is_empty()) {
            Some(slug) => slug.trim().to_string(),
            None => Self::next_slug(pool, &title).await?,
        };

        llm::initialize_project(&path, &slug, template_root, log, mysql_password, is_mariadb).await?;

        let id = sqlx::query("INSERT INTO projects (title, slug, path) VALUES (?, ?, ?)")
            .bind(&title)
            .bind(&slug)
            .bind(&path)
            .execute(pool)
            .await?
            .last_insert_rowid();

        // Mirror the admin account `app:init` seeds into the project's Symfony
        // backend, so the chat login list shows it alongside any users created
        // later.
        UsersRepository::upsert(
            pool,
            &User {
                id: 0,
                project_id: id,
                email: ADMIN_EMAIL.to_string(),
                role: "ROLE_ADMIN".to_string(),
                password: ADMIN_PASSWORD.to_string(),
            },
        )
        .await?;

        Ok(Project {
            id,
            title,
            slug,
            path,
        })
    }

    pub async fn delete(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM projects WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn start(
        pool: &SqlitePool,
        id: i64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let path = sqlx::query_scalar::<_, String>("SELECT path FROM projects WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await?;

        llm::start_project(&path).await
    }

    async fn next_slug(pool: &SqlitePool, title: &str) -> Result<String, sqlx::Error> {
        let base = slugify(title);
        let mut candidate = base.clone();
        let mut suffix = 2;

        while sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM projects WHERE slug = ?")
            .bind(&candidate)
            .fetch_one(pool)
            .await?
            > 0
        {
            candidate = format!("{base}-{suffix}");
            suffix += 1;
        }

        Ok(candidate)
    }
}

/// Data-access layer for the `users` table (project sign-in accounts).
pub struct UsersRepository;

impl UsersRepository {
    pub async fn list(pool: &SqlitePool, project_id: i64) -> Result<Vec<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            "SELECT id, project_id, email, role, password FROM users WHERE project_id = ? ORDER BY (role = 'ROLE_ADMIN') DESC, email ASC",
        )
        .bind(project_id)
        .fetch_all(pool)
        .await
    }

    /// Inserts a user, or updates the existing row when the email already
    /// exists for the project (matching on `(project_id, email)`).
    pub async fn upsert(pool: &SqlitePool, user: &User) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO users (project_id, email, role, password) VALUES (?, ?, ?, ?)
             ON CONFLICT (project_id, email) DO UPDATE SET role = excluded.role, password = excluded.password",
        )
        .bind(user.project_id)
        .bind(&user.email)
        .bind(&user.role)
        .bind(&user.password)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn delete(
        pool: &SqlitePool,
        project_id: i64,
        email: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM users WHERE project_id = ? AND email = ?")
            .bind(project_id)
            .bind(email)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Resolves the owning project id from a project directory path.
    pub async fn project_id_for_path(
        pool: &SqlitePool,
        path: &str,
    ) -> Result<Option<i64>, sqlx::Error> {
        sqlx::query_scalar::<_, i64>("SELECT id FROM projects WHERE path = ?")
            .bind(path)
            .fetch_optional(pool)
            .await
    }
}

/// Data-access layer for the `max_tasks` table (Max mode task lists).
pub struct MaxTasksRepository;

impl MaxTasksRepository {
    /// Replaces the stored task list for a session with a freshly generated one.
    ///
    /// Any previous tasks for the session are dropped first, so re-running the
    /// breakdown phase always starts from a clean slate.
    pub async fn replace_for_session(
        pool: &SqlitePool,
        project_id: i64,
        session_id: i64,
        tasks: &[(String, String)],
    ) -> Result<Vec<MaxTask>, sqlx::Error> {
        let mut tx = pool.begin().await?;
        sqlx::query("DELETE FROM max_tasks WHERE project_id = ? AND session_id = ?")
            .bind(project_id)
            .bind(session_id)
            .execute(&mut *tx)
            .await?;
        for (index, (title, description)) in tasks.iter().enumerate() {
            sqlx::query(
                "INSERT INTO max_tasks (project_id, session_id, task_index, title, description, status) VALUES (?, ?, ?, ?, ?, 'pending')",
            )
            .bind(project_id)
            .bind(session_id)
            .bind(index as i64)
            .bind(title)
            .bind(description)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Self::list(pool, project_id, session_id).await
    }

    /// Lists the tasks for a session in execution order.
    pub async fn list(
        pool: &SqlitePool,
        project_id: i64,
        session_id: i64,
    ) -> Result<Vec<MaxTask>, sqlx::Error> {
        sqlx::query_as::<_, MaxTask>(
            "SELECT id, project_id, session_id, task_index, title, description, status, result_summary FROM max_tasks WHERE project_id = ? AND session_id = ? ORDER BY task_index ASC",
        )
        .bind(project_id)
        .bind(session_id)
        .fetch_all(pool)
        .await
    }

    /// Returns the first task that still needs to run (pending or failed), if any.
    pub async fn next_unfinished(
        pool: &SqlitePool,
        project_id: i64,
        session_id: i64,
    ) -> Result<Option<MaxTask>, sqlx::Error> {
        sqlx::query_as::<_, MaxTask>(
            "SELECT id, project_id, session_id, task_index, title, description, status, result_summary FROM max_tasks WHERE project_id = ? AND session_id = ? AND status IN ('pending', 'failed') ORDER BY task_index ASC LIMIT 1",
        )
        .bind(project_id)
        .bind(session_id)
        .fetch_optional(pool)
        .await
    }

    /// Updates the status (and optionally the result summary) of a single task.
    pub async fn set_status(
        pool: &SqlitePool,
        id: i64,
        status: &str,
        result_summary: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE max_tasks SET status = ?, result_summary = COALESCE(?, result_summary), updated_at = CURRENT_TIMESTAMP WHERE id = ?",
        )
        .bind(status)
        .bind(result_summary)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Marks every failed task back to pending so the sequence can be retried.
    pub async fn reset_failed(
        pool: &SqlitePool,
        project_id: i64,
        session_id: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE max_tasks SET status = 'pending', updated_at = CURRENT_TIMESTAMP WHERE project_id = ? AND session_id = ? AND status = 'failed'",
        )
        .bind(project_id)
        .bind(session_id)
        .execute(pool)
        .await?;
        Ok(())
    }
}

pub struct SessionsRepository;

impl SessionsRepository {
    pub async fn list(pool: &SqlitePool, project_id: i64) -> Result<Vec<Session>, sqlx::Error> {
        sqlx::query_as::<_, Session>("SELECT id, title, conversation_history, token_usage, compacted_summary FROM sessions WHERE project_id = ? ORDER BY created_at DESC, id DESC")
            .bind(project_id).fetch_all(pool).await
    }

    pub async fn get(pool: &SqlitePool, project_id: i64, id: i64) -> Result<Session, sqlx::Error> {
        sqlx::query_as::<_, Session>("SELECT id, title, conversation_history, token_usage, compacted_summary FROM sessions WHERE id = ? AND project_id = ?")
            .bind(id)
            .bind(project_id)
            .fetch_one(pool)
            .await
    }

    pub async fn compact_session(
        pool: &SqlitePool,
        project_id: i64,
        id: i64,
        compacted_history: String,
        compacted_summary: String,
        token_usage: i64,
    ) -> Result<Session, sqlx::Error> {
        sqlx::query("UPDATE sessions SET conversation_history = ?, compacted_summary = ?, token_usage = ? WHERE id = ? AND project_id = ?")
            .bind(&compacted_history)
            .bind(&compacted_summary)
            .bind(token_usage)
            .bind(id)
            .bind(project_id)
            .execute(pool)
            .await?;
        Self::get(pool, project_id, id).await
    }

    pub async fn delete(pool: &SqlitePool, project_id: i64, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM sessions WHERE id = ? AND project_id = ?")
            .bind(id)
            .bind(project_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn save_exchange(
        pool: &SqlitePool,
        project_id: i64,
        session_id: Option<i64>,
        user_message: String,
        artifacts: Vec<AgentArtifact>,
        agent: &AgentService,
        event_sink: Arc<dyn Fn(Value) + Send + Sync>,
        mode: Option<String>,
        app_handle: Option<&AppHandle>,
        cancel_token: Option<Arc<std::sync::atomic::AtomicBool>>,
        iteration_control: Option<Arc<crate::iterate::IterationControl>>,
    ) -> Result<Session, Box<dyn std::error::Error + Send + Sync>> {
        let is_plan_mode = mode.as_deref() == Some("plan");
        let existing_history = match session_id {
            Some(id) => {
                sqlx::query_scalar::<_, String>(
                    "SELECT conversation_history FROM sessions WHERE id = ? AND project_id = ?",
                )
                .bind(id)
                .bind(project_id)
                .fetch_one(pool)
                .await?
            }
            None => "[]".to_string(),
        };
        let mut messages: Vec<ChatMessage> =
            serde_json::from_str(&existing_history).unwrap_or_default();
        messages.push(ChatMessage {
            role: "user".to_string(),
            content: user_message.clone(),
            data: mode.as_ref().map(|m| json!({ "mode": m })),
        });
        let id = match session_id {
            Some(id) => {
                let history = serde_json::to_string(&messages)?;
                sqlx::query("UPDATE sessions SET last_message = ?, conversation_history = ? WHERE id = ? AND project_id = ?")
                    .bind(&user_message).bind(history).bind(id).bind(project_id).execute(pool).await?;
                id
            }
            None => {
                let history = serde_json::to_string(&messages)?;
                sqlx::query("INSERT INTO sessions (project_id, title, last_message, conversation_history) VALUES (?, 'New chat', ?, ?)")
                    .bind(project_id).bind(&user_message).bind(history).execute(pool).await?.last_insert_rowid()
            }
        };

        let is_qa_mode = mode.as_deref() == Some("qa");
        let is_iterate_mode = mode.as_deref() == Some("iterate");
        let is_max_mode = mode.as_deref() == Some("max");

        if is_iterate_mode {
            let control = iteration_control.unwrap_or_else(|| Arc::new(crate::iterate::IterationControl::new()));
            return crate::iterate::run_iterate_loop(
                pool,
                project_id,
                id,
                user_message,
                artifacts,
                agent,
                event_sink,
                app_handle,
                control,
            )
            .await;
        }

        if is_max_mode {
            return crate::max::run_max_breakdown(
                pool,
                project_id,
                id,
                user_message,
                artifacts,
                agent,
                event_sink,
                app_handle,
            )
            .await;
        }

        let (system_prompt, tools) = if is_plan_mode {
            let app = llm::PlanningApp;
            (app.system_prompt(), app.tools())
        } else if is_qa_mode {
            let app = llm::QaApp;
            (app.system_prompt(), app.tools())
        } else {
            let app = llm::AdminPanelApp;
            (app.system_prompt(), app.tools())
        };

        let project_path = sqlx::query_scalar::<_, String>("SELECT path FROM projects WHERE id = ?")
            .bind(project_id)
            .fetch_one(pool)
            .await?;
        let streamed_messages = Arc::new(Mutex::new(Vec::new()));
        let callback_messages = Arc::clone(&streamed_messages);
        let callback_sink = Arc::clone(&event_sink);
        let log_dir = app_handle
            .and_then(|handle| handle.path().app_log_dir().ok().or_else(|| handle.path().app_data_dir().ok()))
            .map(|path| path.join("agent-conversations").to_string_lossy().to_string());

        let result = agent
            .run_agent(
                conversation_messages(&messages, &system_prompt),
                tools,
                AgentOptions {
                    project_path: Some(project_path),
                    artifacts,
                    log_dir,
                    on_event: Some(Arc::new(move |event| {
                        let event_type =
                            event.get("type").and_then(Value::as_str).unwrap_or("event");
                        if event_type != "complete" && event_type != "thought_delta" && event_type != "content_delta" {
                            callback_messages
                                .lock()
                                .expect("agent event lock poisoned")
                                .push(ChatMessage {
                                    role: event_type.to_string(),
                                    content: event_content(&event),
                                    data: event.get("data").cloned(),
                                });
                        }
                        callback_sink(
                            json!({ "projectId": project_id, "sessionId": id, "event": event }),
                        );
                    })),
                    cancellation_token: cancel_token,
                    ..Default::default()
                },
            )
            .await?;

        messages.extend(
            streamed_messages
                .lock()
                .expect("agent event lock poisoned")
                .clone(),
        );

        let mut assistant_data: Option<Value> = None;
        if is_plan_mode {
            let plan_path_str = if let Some(handle) = app_handle {
                if let Ok(data_dir) = handle.path().app_data_dir() {
                    let plans_dir = data_dir.join("plans");
                    let _ = std::fs::create_dir_all(&plans_dir);
                    let plan_file = plans_dir.join(format!("project_{}_session_{}_plan.md", project_id, id));
                    let _ = std::fs::write(&plan_file, &result.content);
                    Some(plan_file.to_string_lossy().to_string())
                } else {
                    None
                }
            } else {
                None
            };
            assistant_data = Some(json!({
                "isPlan": true,
                "planPath": plan_path_str
            }));
        }

        messages.push(ChatMessage {
            role: "assistant".to_string(),
            content: result.content,
            data: assistant_data,
        });
        let history = serde_json::to_string(&messages)?;
        let title = messages
            .iter()
            .find(|message| message.role == "user")
            .map(|message| message.content.chars().take(48).collect::<String>())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| "New chat".to_string());
        let token_usage_val = result.token_usage.unwrap_or(0) as i64;
        let mut update_query = "UPDATE sessions SET title = ?, conversation_history = ?".to_string();
        if token_usage_val > 0 {
            update_query.push_str(", token_usage = ?");
            sqlx::query(&format!("{update_query} WHERE id = ? AND project_id = ?"))
                .bind(&title).bind(&history).bind(token_usage_val).bind(id).bind(project_id).execute(pool).await?;
        } else {
            sqlx::query(&format!("{update_query} WHERE id = ? AND project_id = ?"))
                .bind(&title).bind(&history).bind(id).bind(project_id).execute(pool).await?;
        }
        let (current_token_usage, current_compacted): (i64, Option<String>) = sqlx::query_as("SELECT token_usage, compacted_summary FROM sessions WHERE id = ? AND project_id = ?")
            .bind(id)
            .bind(project_id)
            .fetch_one(pool)
            .await
            .unwrap_or((token_usage_val, None));

        Ok(Session {
            id,
            title,
            conversation_history: history,
            token_usage: current_token_usage,
            compacted_summary: current_compacted,
        })
    }
}

fn conversation_messages(history: &[ChatMessage], system_prompt: &str) -> Vec<LLMMessage> {
    let mut messages = vec![LLMMessage {
        role: LLMMessageRole::System,
        content: system_prompt.to_string(),
        tool_call_id: None,
        tool_name: None,
        tool_args: None,
        tool_calls: None,
    }];
    let mut last_call_id = None;
    for (index, message) in history.iter().enumerate() {
        match message.role.as_str() {
            "user" => {
                messages.push(LLMMessage {
                    role: LLMMessageRole::User,
                    content: message.content.clone(),
                    tool_call_id: None,
                    tool_name: None,
                    tool_args: None,
                    tool_calls: None,
                });
            }
            "tool_call" => {
                let (call_id, name, args) = if let Some(data) = &message.data {
                    let id = data
                        .get("id")
                        .and_then(Value::as_str)
                        .map(String::from)
                        .unwrap_or_else(|| format!("call_{index}"));
                    let name = data
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("tool")
                        .to_string();
                    let raw_args = data.get("arguments").cloned().unwrap_or(Value::Null);
                    let args_str = match raw_args {
                        Value::String(s) => s,
                        Value::Null => "{}".to_string(),
                        other => other.to_string(),
                    };
                    (id, name, args_str)
                } else {
                    (format!("call_{index}"), "tool".to_string(), "{}".to_string())
                };
                last_call_id = Some(call_id.clone());
                messages.push(LLMMessage {
                    role: LLMMessageRole::Assistant,
                    content: String::new(),
                    tool_call_id: None,
                    tool_name: None,
                    tool_args: None,
                    tool_calls: Some(vec![json!({
                        "id": call_id,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": args,
                        }
                    })]),
                });
            }
            "tool_result" => {
                let (call_id, name, content) = if let Some(data) = &message.data {
                    let id = data
                        .get("id")
                        .and_then(Value::as_str)
                        .map(String::from)
                        .or_else(|| last_call_id.take())
                        .unwrap_or_else(|| format!("call_{index}"));
                    let name = data
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("tool")
                        .to_string();
                    let result = data.get("result").cloned().unwrap_or(Value::Null);
                    let content_str = if let Some(s) = result.as_str() {
                        s.to_string()
                    } else if !result.is_null() {
                        result.to_string()
                    } else {
                        message.content.clone()
                    };
                    (id, name, content_str)
                } else {
                    let id = last_call_id.take().unwrap_or_else(|| format!("call_{index}"));
                    (id, "tool".to_string(), message.content.clone())
                };
                messages.push(LLMMessage {
                    role: LLMMessageRole::Tool,
                    content,
                    tool_call_id: Some(call_id),
                    tool_name: Some(name),
                    tool_args: None,
                    tool_calls: None,
                });
            }
            "assistant" => {
                if !message.content.trim().is_empty() {
                    messages.push(LLMMessage {
                        role: LLMMessageRole::Assistant,
                        content: message.content.clone(),
                        tool_call_id: None,
                        tool_name: None,
                        tool_args: None,
                        tool_calls: None,
                    });
                }
            }
            _ => {}
        }
    }
    messages
}

fn event_content(event: &Value) -> String {
    let data = event.get("data").cloned().unwrap_or(Value::Null);
    match event.get("type").and_then(Value::as_str) {
        Some("thought") => data
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        Some("tool_call") => format!(
            "{}({})",
            data.get("name").and_then(Value::as_str).unwrap_or("tool"),
            data.get("arguments").cloned().unwrap_or(Value::Null)
        ),
        Some("tool_result") => format!(
            "{} returned {}",
            data.get("name").and_then(Value::as_str).unwrap_or("tool"),
            data.get("result").cloned().unwrap_or(Value::Null)
        ),
        _ => data.to_string(),
    }
}

fn slugify(title: &str) -> String {
    let slug = title
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let slug = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "project".to_string()
    } else {
        slug
    }
}

pub async fn initialize(app: &AppHandle) -> Result<SqlitePool, Box<dyn std::error::Error>> {
    let data_directory = app.path().app_data_dir()?;

    std::fs::create_dir_all(&data_directory)?;

    let options = SqliteConnectOptions::new()
        .filename(data_directory.join("eggshell.db"))
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePool::connect_with(options).await?;

    MIGRATOR.run(&pool).await?;

    // Defensive check: ensure newly added columns exist in older SQLite databases
    let _ = sqlx::query("ALTER TABLE sessions ADD COLUMN token_usage INTEGER NOT NULL DEFAULT 0")
        .execute(&pool)
        .await;
    let _ = sqlx::query("ALTER TABLE sessions ADD COLUMN compacted_summary TEXT")
        .execute(&pool)
        .await;

    set_pool(pool.clone());

    Ok(pool)
}
