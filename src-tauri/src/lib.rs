mod db;
pub mod config;
pub mod llm;
mod tools;

use db::{NewProject, Project, ProjectsRepository, Session, SessionsRepository};
use sqlx::SqlitePool;
use serde::Serialize;
use std::net::{SocketAddr, TcpStream};
use std::process::Command;
use std::sync::Arc;
use tauri::Emitter;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[derive(Debug, Serialize)]
struct DependencyStatus { node: bool, php: bool, symfony: bool, mysql: bool }

fn executable_in_path(executable: &str) -> bool {
    let locator = if cfg!(windows) { "where" } else { "which" };
    let result = Command::new(locator).arg(executable).output();
    match result {
        Ok(output) => {
            let found = output.status.success();
            found
        }
        Err(error) => {
            false
        }
    }
}

#[tauri::command]
fn detect_dependencies() -> DependencyStatus {
    let node = executable_in_path("node");
    let php = executable_in_path("php");
    let symfony = executable_in_path("symfony");
    let mysql = executable_in_path("mysql");
    let mysqld = if mysql { false } else { executable_in_path("mysqld") };
    let status = DependencyStatus { node, php, symfony, mysql: mysql || mysqld };
    status
}

#[tauri::command]
async fn list_projects(pool: tauri::State<'_, SqlitePool>) -> Result<Vec<Project>, String> {
    ProjectsRepository::list(pool.inner())
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn create_project(
    project: NewProject,
    pool: tauri::State<'_, SqlitePool>,
) -> Result<Project, String> {
    if project.title.trim().is_empty() || project.path.trim().is_empty() {
        return Err("A project title and folder are required.".to_string());
    }

    ProjectsRepository::create(pool.inner(), project)
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
async fn list_sessions(project_id: i64, pool: tauri::State<'_, SqlitePool>) -> Result<Vec<Session>, String> {
    SessionsRepository::list(pool.inner(), project_id).await.map_err(|error| error.to_string())
}

#[tauri::command]
async fn delete_session(project_id: i64, id: i64, pool: tauri::State<'_, SqlitePool>) -> Result<(), String> {
    SessionsRepository::delete(pool.inner(), project_id, id).await.map_err(|error| error.to_string())
}

#[tauri::command]
async fn send_message(project_id: i64, session_id: Option<i64>, message: String, app: tauri::AppHandle, pool: tauri::State<'_, SqlitePool>, agent: tauri::State<'_, llm::AgentService>) -> Result<Session, String> {
    let message = message.trim().to_string();
    if message.is_empty() { return Err("A message is required.".to_string()); }
    let event_sink = Arc::new(move |payload| { let _ = app.emit("agent-event", payload); });
    SessionsRepository::save_exchange(pool.inner(), project_id, session_id, message, agent.inner(), event_sink).await.map_err(|error| error.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let pool = tauri::async_runtime::block_on(db::initialize())
        .expect("failed to initialize SQLite database");
    let config = config::ConfigService::load_default().expect("failed to load config.yaml");
    let agent = llm::AgentService::new(Arc::new(llm::OllamaService::new(config.ollama)));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(pool)
        .manage(agent)
        .invoke_handler(tauri::generate_handler![
            greet,
            detect_dependencies,
            list_projects,
            create_project,
            delete_project,
            start_project,
            list_sessions,
            delete_session,
            send_message
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
