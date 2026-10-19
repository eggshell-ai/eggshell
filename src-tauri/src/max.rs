use crate::db::{ChatMessage, MaxTask, MaxTasksRepository, Session, SessionsRepository};
use crate::llm::{
    AdminPanelApp, AgentArtifact, AgentOptions, AgentService, App, LLMMessage, LLMMessageRole,
    MaxPlannerApp,
};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Manager};

/// State control for an active Max mode run.
///
/// Max mode runs a sequence of independent agent tasks. Unlike iterate, there
/// is no wrap-up phase: stopping simply aborts the current task and leaves the
/// remaining tasks pending so the run can be resumed later.
#[derive(Debug, Clone)]
pub struct MaxControl {
    /// When true, the running LLM turn is aborted and the sequence halts.
    pub force_stop: Arc<AtomicBool>,
}

impl MaxControl {
    pub fn new() -> Self {
        Self {
            force_stop: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Default for MaxControl {
    fn default() -> Self {
        Self::new()
    }
}

/// Runs Phase 1 of Max mode: break the user's requirement into a simple,
/// business-oriented task list, persist it, and return the updated session.
///
/// This does not execute any task. The frontend renders the task list and the
/// user starts Phase 2 (execution) explicitly.
pub async fn run_max_breakdown(
    pool: &SqlitePool,
    project_id: i64,
    session_id: i64,
    user_message: String,
    artifacts: Vec<AgentArtifact>,
    agent: &AgentService,
    event_sink: Arc<dyn Fn(Value) + Send + Sync>,
    app_handle: Option<&AppHandle>,
) -> Result<Session, Box<dyn std::error::Error + Send + Sync>> {
    let project_path = sqlx::query_scalar::<_, String>("SELECT path FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_one(pool)
        .await?;

    let log_dir = max_log_dir(app_handle);

    let existing_history = sqlx::query_scalar::<_, String>(
        "SELECT conversation_history FROM sessions WHERE id = ? AND project_id = ?",
    )
    .bind(session_id)
    .bind(project_id)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|_| "[]".to_string());

    let mut messages: Vec<ChatMessage> =
        serde_json::from_str(&existing_history).unwrap_or_default();

    let app = MaxPlannerApp;
    let planner_messages = vec![
        LLMMessage {
            role: LLMMessageRole::System,
            content: app.system_prompt(),
            tool_call_id: None,
            tool_name: None,
            tool_args: None,
            tool_calls: None,
        },
        LLMMessage {
            role: LLMMessageRole::User,
            content: format!(
                "Break the following requirement into a simple, business-oriented task list.\n\n{}",
                user_message
            ),
            tool_call_id: None,
            tool_name: None,
            tool_args: None,
            tool_calls: None,
        },
    ];

    let event_sink_clone = Arc::clone(&event_sink);
    let result = agent
        .run_agent(
            planner_messages,
            app.tools(),
            AgentOptions {
                project_path: Some(project_path.clone()),
                artifacts: artifacts.clone(),
                log_dir,
                // The planner works without skills: it must produce a purely
                // business-oriented plan, so no catalogue is appended to its
                // system prompt.
                skills: Some(Vec::new()),
                on_event: Some(Arc::new(move |event| {
                    event_sink_clone(json!({
                        "projectId": project_id,
                        "sessionId": session_id,
                        "event": event,
                        "maxStage": "breakdown"
                    }));
                })),
                max_turns: Some(5),
                ..Default::default()
            },
        )
        .await?;

    let tasks = parse_tasks(&result.content);
    let stored = MaxTasksRepository::replace_for_session(pool, project_id, session_id, &tasks).await?;

    messages.push(ChatMessage {
        role: "assistant".to_string(),
        content: result.content.clone(),
        data: Some(json!({
            "isMaxPlan": true,
            "maxTaskCount": stored.len(),
            "maxTasks": stored,
        })),
    });

    let history = serde_json::to_string(&messages)?;
    let title = user_message.chars().take(48).collect::<String>();
    let token_usage_val = result.token_usage.unwrap_or(0) as i64;
    sqlx::query(
        "UPDATE sessions SET title = ?, conversation_history = ?, token_usage = MAX(token_usage, ?) WHERE id = ? AND project_id = ?",
    )
    .bind(if title.is_empty() { "New chat".to_string() } else { title })
    .bind(&history)
    .bind(token_usage_val)
    .bind(session_id)
    .bind(project_id)
    .execute(pool)
    .await?;

    event_sink(json!({
        "projectId": project_id,
        "sessionId": session_id,
        "event": {
            "type": "max_breakdown_complete",
            "data": { "taskCount": stored.len(), "tasks": stored }
        }
    }));

    Ok(SessionsRepository::get(pool, project_id, session_id).await?)
}

/// Runs Phase 2 of Max mode: execute each unfinished task sequentially, one
/// agent per task, updating the stored task list and session as it goes.
///
/// On the first task failure the sequence halts (remaining tasks stay pending)
/// so the user can retry, skip, or resume.
pub async fn run_max_execution(
    pool: &SqlitePool,
    project_id: i64,
    session_id: i64,
    agent: &AgentService,
    event_sink: Arc<dyn Fn(Value) + Send + Sync>,
    app_handle: Option<&AppHandle>,
    control: Arc<MaxControl>,
) -> Result<Session, Box<dyn std::error::Error + Send + Sync>> {
    let project_path = sqlx::query_scalar::<_, String>("SELECT path FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_one(pool)
        .await?;

    let log_dir = max_log_dir(app_handle);

    let existing_history = sqlx::query_scalar::<_, String>(
        "SELECT conversation_history FROM sessions WHERE id = ? AND project_id = ?",
    )
    .bind(session_id)
    .bind(project_id)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|_| "[]".to_string());

    let mut messages: Vec<ChatMessage> =
        serde_json::from_str(&existing_history).unwrap_or_default();

    // The overall goal is the first user message in the session.
    let overall_goal = messages
        .iter()
        .find(|message| message.role == "user")
        .map(|message| message.content.clone())
        .unwrap_or_default();

    let all_tasks = MaxTasksRepository::list(pool, project_id, session_id).await?;
    if all_tasks.is_empty() {
        return Err("No task list found for this session. Run the Max breakdown first.".into());
    }
    let task_titles: Vec<String> = all_tasks.iter().map(|task| task.title.clone()).collect();
    let total = all_tasks.len();

    let app = AdminPanelApp;
    let mut total_session_tokens: i64 = 0;

    event_sink(json!({
        "projectId": project_id,
        "sessionId": session_id,
        "event": {
            "type": "max_execution_start",
            "data": { "taskCount": total }
        }
    }));

    loop {
        if control.force_stop.load(Ordering::Relaxed) {
            event_sink(json!({
                "projectId": project_id,
                "sessionId": session_id,
                "event": {
                    "type": "max_status",
                    "data": { "phase": "stopped" }
                }
            }));
            break;
        }

        let Some(task) = MaxTasksRepository::next_unfinished(pool, project_id, session_id).await?
        else {
            event_sink(json!({
                "projectId": project_id,
                "sessionId": session_id,
                "event": {
                    "type": "max_status",
                    "data": { "phase": "completed" }
                }
            }));
            break;
        };

        event_sink(json!({
            "projectId": project_id,
            "sessionId": session_id,
            "event": {
                "type": "max_task_start",
                "data": { "taskId": task.id, "taskIndex": task.task_index, "title": task.title, "total": total }
            }
        }));

        MaxTasksRepository::set_status(pool, task.id, "running", None).await?;

        let task_messages = vec![
            LLMMessage {
                role: LLMMessageRole::System,
                content: app.system_prompt(),
                tool_call_id: None,
                tool_name: None,
                tool_args: None,
                tool_calls: None,
            },
            LLMMessage {
                role: LLMMessageRole::User,
                content: task_agent_prompt(&overall_goal, &task_titles, &task),
                tool_call_id: None,
                tool_name: None,
                tool_args: None,
                tool_calls: None,
            },
        ];

        let event_sink_clone = Arc::clone(&event_sink);
        let run_result = agent
            .run_agent(
                task_messages,
                app.tools(),
                AgentOptions {
                    context: None,
                    project_path: Some(project_path.clone()),
                    artifacts: Vec::new(),
                    log_dir: log_dir.clone(),
                    on_event: Some(Arc::new(move |event| {
                        event_sink_clone(json!({
                            "projectId": project_id,
                            "sessionId": session_id,
                            "event": event,
                            "maxStage": "task",
                            "maxTaskId": task.id
                        }));
                    })),
                    cancellation_token: Some(Arc::clone(&control.force_stop)),
                    max_turns: Some(15),
                    ..Default::default()
                },
            )
            .await;

        match run_result {
            Ok(outcome) => {
                if let Some(tokens) = outcome.token_usage {
                    total_session_tokens += tokens as i64;
                }

                // A stopped run should leave the task resumable rather than
                // marking it completed on a truncated result.
                if control.force_stop.load(Ordering::Relaxed) {
                    MaxTasksRepository::set_status(pool, task.id, "pending", None).await?;
                    event_sink(json!({
                        "projectId": project_id,
                        "sessionId": session_id,
                        "event": {
                            "type": "max_status",
                            "data": { "phase": "stopped" }
                        }
                    }));
                    break;
                }

                MaxTasksRepository::set_status(
                    pool,
                    task.id,
                    "completed",
                    Some(&truncate(&outcome.content, 2000)),
                )
                .await?;

                messages.push(ChatMessage {
                    role: "assistant".to_string(),
                    content: format!("### Task {} of {} complete: {}\n\n{}", task.task_index + 1, total, task.title, outcome.content),
                    data: Some(json!({
                        "isMaxTaskResult": true,
                        "taskId": task.id,
                        "taskIndex": task.task_index,
                        "title": task.title,
                    })),
                });

                event_sink(json!({
                    "projectId": project_id,
                    "sessionId": session_id,
                    "event": {
                        "type": "max_task_complete",
                        "data": {
                            "taskId": task.id,
                            "taskIndex": task.task_index,
                            "title": task.title,
                            "total": total,
                            "summary": truncate(&outcome.content, 2000)
                        }
                    }
                }));
            }
            Err(error) => {
                let message = error.to_string();
                MaxTasksRepository::set_status(pool, task.id, "failed", Some(&truncate(&message, 2000))).await?;
                messages.push(ChatMessage {
                    role: "assistant".to_string(),
                    content: format!("### Task {} of {} failed: {}\n\n{}", task.task_index + 1, total, task.title, message),
                    data: Some(json!({
                        "isMaxTaskResult": true,
                        "taskId": task.id,
                        "taskIndex": task.task_index,
                        "title": task.title,
                        "failed": true,
                    })),
                });
                event_sink(json!({
                    "projectId": project_id,
                    "sessionId": session_id,
                    "event": {
                        "type": "max_task_failed",
                        "data": {
                            "taskId": task.id,
                            "taskIndex": task.task_index,
                            "title": task.title,
                            "total": total,
                            "error": truncate(&message, 2000)
                        }
                    }
                }));
                // Halt the sequence: the user decides whether to retry or skip.
                event_sink(json!({
                    "projectId": project_id,
                    "sessionId": session_id,
                    "event": {
                        "type": "max_status",
                        "data": { "phase": "halted" }
                    }
                }));
                break;
            }
        }

        persist_progress(
            pool,
            project_id,
            session_id,
            &messages,
            total_session_tokens,
        )
        .await?;
    }

    persist_progress(
        pool,
        project_id,
        session_id,
        &messages,
        total_session_tokens,
    )
    .await?;

    let tasks = MaxTasksRepository::list(pool, project_id, session_id).await?;
    event_sink(json!({
        "projectId": project_id,
        "sessionId": session_id,
        "event": {
            "type": "max_execution_complete",
            "data": { "tasks": tasks }
        }
    }));

    Ok(SessionsRepository::get(pool, project_id, session_id).await?)
}

/// Writes the current task list to the session's conversation history and token
/// usage, so a long run survives a restart mid-way.
async fn persist_progress(
    pool: &SqlitePool,
    project_id: i64,
    session_id: i64,
    messages: &[ChatMessage],
    token_usage: i64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let history = serde_json::to_string(messages)?;
    sqlx::query("UPDATE sessions SET conversation_history = ?, token_usage = MAX(token_usage, ?) WHERE id = ? AND project_id = ?")
        .bind(&history)
        .bind(token_usage)
        .bind(session_id)
        .bind(project_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Builds the per-task prompt for an implementer agent.
///
/// Each agent gets the overall goal, the full ordered task list (so it knows
/// where its chunk fits), and its single task. It is told to re-read the
/// relevant skills and schema before implementing.
fn task_agent_prompt(overall_goal: &str, all_titles: &[String], task: &MaxTask) -> String {
    let task_list = all_titles
        .iter()
        .enumerate()
        .map(|(index, title)| {
            if title == &task.title && index as i64 == task.task_index {
                format!("{}. {}  <-- YOUR TASK", index + 1, title)
            } else {
                format!("{}. {}", index + 1, title)
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "You are building one part of a larger application.\n\n\
=== OVERALL PROJECT GOAL ===\n{}\n\n\
=== FULL TASK LIST ===\n{}\n\n\
=== YOUR TASK ===\n{}\n{}\n\n\
=== INSTRUCTIONS ===\n\
- Implement ONLY your task above. Do not start or scaffold the other tasks.\n\
- Before writing any code, inspect the existing project: read `frontend/src/menu-items/menu.json` and use `read_schema` to see the current database schema, so what you build matches what already exists.\n\
- Re-read the relevant skills in full with `load_skill` (e.g. crud_creation for a CRUD screen, analytics_and_reporting for a report or dashboard) and follow their instructions exactly.\n\
- Keep your change small and self-contained: this task should be one screen, CRUD, report, or dashboard.\n\
- Run `lint_code` after making changes to verify there are no syntax, lint, or schema errors.\n\
- When done, briefly summarise what you created and any follow-up the other task agents will need.",
        overall_goal, task_list, task.title, task.description
    )
}

/// Parses the Max planner's response into `(title, description)` pairs.
///
/// Prefers the fenced JSON array the planner is instructed to emit; falls back
/// to the numbered Markdown list when the JSON is missing or malformed.
pub fn parse_tasks(content: &str) -> Vec<(String, String)> {
    if let Some(tasks) = parse_tasks_from_json(content) {
        if !tasks.is_empty() {
            return tasks;
        }
    }
    parse_tasks_from_lines(content)
}

/// Extracts tasks from the first fenced ```json block that parses to an array
/// of objects with a `title` (and optional `description`).
fn parse_tasks_from_json(content: &str) -> Option<Vec<(String, String)>> {
    let mut rest = content;
    while let Some(start) = rest.find("```") {
        let after_fence = &rest[start + 3..];
        // Skip an optional language tag such as `json` on the fence line.
        let body_start = after_fence.find('\n').map(|offset| offset + 1).unwrap_or(0);
        let body = &after_fence[body_start..];
        let Some(end) = body.find("```") else { break };
        let candidate = body[..end].trim();
        rest = &body[end + 3..];

        if !candidate.starts_with('[') {
            continue;
        }

        if let Ok(Value::Array(items)) = serde_json::from_str::<Value>(candidate) {
            let tasks: Vec<(String, String)> = items
                .iter()
                .filter_map(|item| {
                    let title = item.get("title").and_then(Value::as_str)?.trim().to_string();
                    if title.is_empty() {
                        return None;
                    }
                    let description = item
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                    Some((title, description))
                })
                .collect();
            if !tasks.is_empty() {
                return Some(tasks);
            }
        }
    }
    None
}

/// Fallback: reads numbered lines (`1. Title — Description`) under the Tasks
/// heading, tolerating `-`, `—`, `:` or `–` as the title/description separator.
fn parse_tasks_from_lines(content: &str) -> Vec<(String, String)> {
    let mut tasks = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        let Some(rest) = strip_numbered_prefix(trimmed) else {
            continue;
        };
        let cleaned = rest.trim_start_matches(['*', '-', ' ']).trim();
        if cleaned.is_empty() {
            continue;
        }
        let (title, description) = split_title_description(cleaned);
        if !title.is_empty() {
            tasks.push((title, description));
        }
    }
    tasks
}

/// Returns the text after a leading `N.` / `N)` marker, if present.
fn strip_numbered_prefix(line: &str) -> Option<&str> {
    let mut chars = line.char_indices();
    let mut digits = 0;
    for (_, character) in chars.by_ref() {
        if character.is_ascii_digit() {
            digits += 1;
        } else if (character == '.' || character == ')') && digits > 0 {
            let offset = line
                .find(|c: char| c == '.' || c == ')')
                .map(|position| position + 1)
                .unwrap_or(0);
            return Some(&line[offset..]);
        } else {
            return None;
        }
    }
    None
}

fn split_title_description(text: &str) -> (String, String) {
    for separator in [" — ", " – ", " - ", " | ", ": "] {
        if let Some(index) = text.find(separator) {
            let title = text[..index].trim().to_string();
            let description = text[index + separator.len()..].trim().to_string();
            if !title.is_empty() {
                return (title, description);
            }
        }
    }
    (text.trim().to_string(), String::new())
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_chars).collect();
    format!("{truncated}…")
}

/// Resolves the agent-conversation log directory, mirroring iterate mode.
fn max_log_dir(app_handle: Option<&AppHandle>) -> Option<String> {
    app_handle
        .and_then(|handle| {
            handle
                .path()
                .app_log_dir()
                .ok()
                .or_else(|| handle.path().app_data_dir().ok())
        })
        .map(|path| {
            path.join("agent-conversations")
                .to_string_lossy()
                .to_string()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fenced_json_tasks() {
        let content = "## Tasks\n1. Do a thing\n\n```json\n[{\"title\":\"Create customer CRUD\",\"description\":\"Manage customers\"},{\"title\":\"Create sales report\",\"description\":\"See sales\"}]\n```\n";
        let tasks = parse_tasks(content);
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].0, "Create customer CRUD");
        assert_eq!(tasks[1].1, "See sales");
    }

    #[test]
    fn falls_back_to_numbered_lines() {
        let content = "## Tasks\n1. Create customer CRUD — Manage customers\n2. Create product CRUD — Manage products\n- not a task\n";
        let tasks = parse_tasks(content);
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].0, "Create customer CRUD");
        assert_eq!(tasks[0].1, "Manage customers");
        assert_eq!(tasks[1].0, "Create product CRUD");
    }

    #[test]
    fn ignores_lines_without_numbers() {
        let tasks = parse_tasks("## Overview\nSome prose here.\n");
        assert!(tasks.is_empty());
    }
}
