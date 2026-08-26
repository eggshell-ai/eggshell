use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{sqlite::SqliteConnectOptions, FromRow, SqlitePool};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};

use crate::llm::{self, AgentOptions, AgentService, App, LLMMessage, LLMMessageRole};
use crate::progress::ProgressLog;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

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
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
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
    ) -> Result<Project, Box<dyn std::error::Error + Send + Sync>> {
        let NewProject { title, slug, path } = project;
        let title = title.trim().to_string();
        let path = path.trim().to_string();
        let slug = match slug.filter(|slug| !slug.trim().is_empty()) {
            Some(slug) => slug.trim().to_string(),
            None => Self::next_slug(pool, &title).await?,
        };

        llm::initialize_project(&path, &slug, template_root, log, mysql_password).await?;

        let id = sqlx::query("INSERT INTO projects (title, slug, path) VALUES (?, ?, ?)")
            .bind(&title)
            .bind(&slug)
            .bind(&path)
            .execute(pool)
            .await?
            .last_insert_rowid();

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

pub struct SessionsRepository;

impl SessionsRepository {
    pub async fn list(pool: &SqlitePool, project_id: i64) -> Result<Vec<Session>, sqlx::Error> {
        sqlx::query_as::<_, Session>("SELECT id, title, conversation_history FROM sessions WHERE project_id = ? ORDER BY created_at DESC, id DESC")
            .bind(project_id).fetch_all(pool).await
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
        agent: &AgentService,
        event_sink: Arc<dyn Fn(Value) + Send + Sync>,
    ) -> Result<Session, Box<dyn std::error::Error + Send + Sync>> {
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
            data: None,
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

        let app = llm::AdminPanelApp;
        let streamed_messages = Arc::new(Mutex::new(Vec::new()));
        let callback_messages = Arc::clone(&streamed_messages);
        let callback_sink = Arc::clone(&event_sink);
        let result = agent
            .run_agent(
                conversation_messages(&messages, &app.system_prompt()),
                app.tools(),
                AgentOptions {
                    on_event: Some(Box::new(move |event| {
                        let event_type =
                            event.get("type").and_then(Value::as_str).unwrap_or("event");
                        if event_type != "complete" {
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
        messages.push(ChatMessage {
            role: "assistant".to_string(),
            content: result.content,
            data: None,
        });
        let history = serde_json::to_string(&messages)?;
        let title = messages
            .iter()
            .find(|message| message.role == "user")
            .map(|message| message.content.chars().take(48).collect::<String>())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| "New chat".to_string());
        sqlx::query("UPDATE sessions SET title = ?, conversation_history = ? WHERE id = ? AND project_id = ?")
            .bind(&title).bind(&history).bind(id).bind(project_id).execute(pool).await?;
        Ok(Session {
            id,
            title,
            conversation_history: history,
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
    messages.extend(history.iter().filter_map(|message| {
        let role = match message.role.as_str() {
            "user" => LLMMessageRole::User,
            "assistant" => LLMMessageRole::Assistant,
            _ => return None,
        };
        Some(LLMMessage {
            role,
            content: message.content.clone(),
            tool_call_id: None,
            tool_name: None,
            tool_args: None,
            tool_calls: None,
        })
    }));
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
    Ok(pool)
}
