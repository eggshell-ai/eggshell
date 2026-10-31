use crate::db::{ChatMessage, Session};
use crate::llm::{
    default_skills, AdminPanelApp, AgentArtifact, AgentOptions, AgentService, App, LLMMessage,
    LLMMessageRole, QaApp, Tool,
};
use crate::tools::{LoadSkillTool, ReadFileTool, ReadSchemaTool};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Manager};

pub const DEFAULT_MAX_ITERATIONS: usize = 10;

/// State control for an active iteration loop on a project.
#[derive(Debug, Clone)]
pub struct IterationControl {
    /// When true, current loop finishes through QA and then stops gracefully.
    pub wrap_up: Arc<AtomicBool>,
    /// When true, the running LLM turn is immediately aborted.
    pub force_stop: Arc<AtomicBool>,
}

impl IterationControl {
    pub fn new() -> Self {
        Self {
            wrap_up: Arc::new(AtomicBool::new(false)),
            force_stop: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Default for IterationControl {
    fn default() -> Self {
        Self::new()
    }
}

/// System prompt for the Planner agent in Iterate mode.
fn planner_system_prompt() -> &'static str {
    "You are an expert autonomous software architect and planning agent for admin-panel web applications.
Your job is to direct focused, incremental iteration cycles on the user's project.

In this iteration planning role:
- You do NOT have file-modifying tools (read-only inspection).
- In each loop, plan ONLY 1 or 2 high-impact, cohesive additions or fixes.
- If there is QA feedback from a previous iteration with errors or bugs, prioritize fixing those issues first!
- Formulate a clean, concrete, numbered checklist for the Implementer agent.

Format your response strictly as:
## Iteration Objective
Brief 1-2 sentence description of what will be added or fixed this cycle.

## QA Remediation (if applicable)
Specific fixes for bugs, missing routes, or broken forms identified by QA.

## Implementation Checklist
1. Step 1 (e.g. database schema change via sync_schema)
2. Step 2 (e.g. backend Symfony entity/controller updates)
3. Step 3 (e.g. frontend Next.js page at app/(dashboard)/... and update menu-items/menu.json)
4. Step 4 (run lint_code to verify syntax and consistency)"
}

/// Planner agent tools (read-only).
fn planner_tools() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(LoadSkillTool::new(default_skills())),
        Box::new(ReadFileTool::new()),
        Box::new(ReadSchemaTool::new()),
    ]
}

/// Executes the autonomous Iterate mode loop.
pub async fn run_iterate_loop(
    pool: &SqlitePool,
    project_id: i64,
    session_id: i64,
    initial_user_prompt: String,
    artifacts: Vec<AgentArtifact>,
    agent: &AgentService,
    event_sink: Arc<dyn Fn(Value) + Send + Sync>,
    app_handle: Option<&AppHandle>,
    control: Arc<IterationControl>,
) -> Result<Session, Box<dyn std::error::Error + Send + Sync>> {
    let project_path = sqlx::query_scalar::<_, String>("SELECT path FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_one(pool)
        .await?;

    let log_dir = app_handle
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
        });

    // Make sure project servers (Next.js & Symfony) are started so QA can reach localhost:4000
    let _ = crate::llm::start_project(&project_path).await;

    let mut current_loop = 1;
    let mut last_plan: Option<String> = None;
    let mut last_implementation_summary: Option<String> = None;
    let mut last_qa_report: Option<String> = None;
    let mut total_session_tokens: i64 = 0;

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

    while current_loop <= DEFAULT_MAX_ITERATIONS {
        if control.force_stop.load(Ordering::Relaxed) {
            event_sink(json!({
                "projectId": project_id,
                "sessionId": session_id,
                "event": {
                    "type": "iterate_status",
                    "data": { "phase": "force_stopped", "loop": current_loop }
                }
            }));
            break;
        }

        let is_wrap_up = control.wrap_up.load(Ordering::Relaxed);
        event_sink(json!({
            "projectId": project_id,
            "sessionId": session_id,
            "event": {
                "type": "iterate_loop_start",
                "data": {
                    "loop": current_loop,
                    "maxLoops": DEFAULT_MAX_ITERATIONS,
                    "isWrapUp": is_wrap_up
                }
            }
        }));

        // ---------------------------------------------------------------------
        // STAGE 1: PLANNER AGENT
        // ---------------------------------------------------------------------
        event_sink(json!({
            "projectId": project_id,
            "sessionId": session_id,
            "event": {
                "type": "iterate_stage_start",
                "data": { "loop": current_loop, "stage": "planner" }
            }
        }));

        let planner_prompt = if current_loop == 1 {
            format!(
                "User's Overall Project Goal:\n{}\n\nThis is Loop #1. Inspect the existing project schema and menu.json, then design the initial foundational entities, dashboard pages, and menu items to implement first.",
                initial_user_prompt
            )
        } else {
            let qa_context = last_qa_report
                .as_deref()
                .unwrap_or("No previous QA issues noted.");
            let impl_context = last_implementation_summary
                .as_deref()
                .unwrap_or("Previous implementation completed.");
            let plan_context = last_plan
                .as_deref()
                .unwrap_or("Previous plan completed.");
            format!(
                "User's Overall Project Goal:\n{}\n\n=== RECENT PROGRESS (Loop #{}) ===\nPrevious Plan:\n{}\n\nImplementation Summary:\n{}\n\nLatest QA Feedback:\n{}\n=== END PROGRESS ===\n\nPlan the next high-value slice for Loop #{}. If QA detected any critical errors, broken routes, or missing tables, prioritize fixing them!",
                initial_user_prompt,
                current_loop - 1,
                plan_context,
                impl_context,
                qa_context,
                current_loop
            )
        };

        let planner_messages = vec![
            LLMMessage {
                role: LLMMessageRole::System,
                content: planner_system_prompt().to_string(),
                tool_call_id: None,
                tool_name: None,
                tool_args: None,
                tool_calls: None,
            },
            LLMMessage {
                role: LLMMessageRole::User,
                content: planner_prompt,
                tool_call_id: None,
                tool_name: None,
                tool_args: None,
                tool_calls: None,
            },
        ];

        let event_sink_clone = Arc::clone(&event_sink);
        let planner_result = agent
            .run_agent(
                planner_messages,
                planner_tools(),
                AgentOptions {
                    project_path: Some(project_path.clone()),
                    artifacts: artifacts.clone(),
                    log_dir: log_dir.clone(),
                    on_event: Some(Arc::new(move |event| {
                        event_sink_clone(json!({
                            "projectId": project_id,
                            "sessionId": session_id,
                            "event": event,
                            "iterateStage": "planner",
                            "iterateLoop": current_loop
                        }));
                    })),
                    cancellation_token: Some(Arc::clone(&control.force_stop)),
                    max_turns: Some(5),
                    ..Default::default()
                },
            )
            .await?;

        if let Some(tokens) = planner_result.token_usage {
            total_session_tokens += tokens as i64;
        }

        let plan_content = planner_result.content;
        last_plan = Some(plan_content.clone());

        event_sink(json!({
            "projectId": project_id,
            "sessionId": session_id,
            "event": {
                "type": "iterate_stage_complete",
                "data": { "loop": current_loop, "stage": "planner", "output": plan_content }
            }
        }));

        if control.force_stop.load(Ordering::Relaxed) {
            break;
        }

        // ---------------------------------------------------------------------
        // STAGE 2: IMPLEMENTER AGENT
        // ---------------------------------------------------------------------
        event_sink(json!({
            "projectId": project_id,
            "sessionId": session_id,
            "event": {
                "type": "iterate_stage_start",
                "data": { "loop": current_loop, "stage": "implementer" }
            }
        }));

        let implementer_app = AdminPanelApp;
        let implementer_prompt = format!(
            "You are the Implementer Agent for Iteration #{}.\nExecute the following implementation plan:\n\n{}\n\nRemember:\n- Apply schema changes with sync_schema if entities/fields are modified.\n- Create or edit Next.js pages under app/(dashboard)/... and update menu-items/menu.json.\n- Run lint_code to verify everything compiles cleanly.\n- Give a short summary of files touched and features created when done.",
            current_loop, plan_content
        );

        let implementer_messages = vec![
            LLMMessage {
                role: LLMMessageRole::System,
                content: implementer_app.system_prompt(),
                tool_call_id: None,
                tool_name: None,
                tool_args: None,
                tool_calls: None,
            },
            LLMMessage {
                role: LLMMessageRole::User,
                content: implementer_prompt,
                tool_call_id: None,
                tool_name: None,
                tool_args: None,
                tool_calls: None,
            },
        ];

        let event_sink_clone = Arc::clone(&event_sink);
        let implementer_result = agent
            .run_agent(
                implementer_messages,
                implementer_app.tools(),
                AgentOptions {
                    project_path: Some(project_path.clone()),
                    artifacts: artifacts.clone(),
                    log_dir: log_dir.clone(),
                    on_event: Some(Arc::new(move |event| {
                        event_sink_clone(json!({
                            "projectId": project_id,
                            "sessionId": session_id,
                            "event": event,
                            "iterateStage": "implementer",
                            "iterateLoop": current_loop
                        }));
                    })),
                    cancellation_token: Some(Arc::clone(&control.force_stop)),
                    max_turns: Some(30),
                    ..Default::default()
                },
            )
            .await?;

        if let Some(tokens) = implementer_result.token_usage {
            total_session_tokens += tokens as i64;
        }

        let implementation_summary = implementer_result.content;
        last_implementation_summary = Some(implementation_summary.clone());

        event_sink(json!({
            "projectId": project_id,
            "sessionId": session_id,
            "event": {
                "type": "iterate_stage_complete",
                "data": { "loop": current_loop, "stage": "implementer", "output": implementation_summary }
            }
        }));

        if control.force_stop.load(Ordering::Relaxed) {
            break;
        }

        // ---------------------------------------------------------------------
        // STAGE 3: QA AGENT
        // ---------------------------------------------------------------------
        event_sink(json!({
            "projectId": project_id,
            "sessionId": session_id,
            "event": {
                "type": "iterate_stage_start",
                "data": { "loop": current_loop, "stage": "qa" }
            }
        }));

        let qa_app = QaApp;
        let qa_prompt = format!(
            "You are the QA Verification Agent for Iteration #{}.\nThe Implementer has completed the following changes:\n{}\n\nFollow the qa_testing skill instructions:\n1. Read frontend/src/menu-items/menu.json to discover active navigation routes.\n2. Open browser at http://localhost:4000 (log in with admin@dummy-project.com / 12345678 if needed).\n3. Navigate to tested routes, check page rendering, inspect forms and tables, and check for console errors with browser_get_console_errors.\n4. Deliver a structured QA report detailing:\n   - Routes Verified\n   - Functional & Visual Status\n   - Issues Found (Critical / Warning / Minor)\n   - Recommendations for the next iteration.",
            current_loop, implementation_summary
        );

        let qa_messages = vec![
            LLMMessage {
                role: LLMMessageRole::System,
                content: qa_app.system_prompt(),
                tool_call_id: None,
                tool_name: None,
                tool_args: None,
                tool_calls: None,
            },
            LLMMessage {
                role: LLMMessageRole::User,
                content: qa_prompt,
                tool_call_id: None,
                tool_name: None,
                tool_args: None,
                tool_calls: None,
            },
        ];

        let event_sink_clone = Arc::clone(&event_sink);
        let qa_result = agent
            .run_agent(
                qa_messages,
                qa_app.tools(),
                AgentOptions {
                    project_path: Some(project_path.clone()),
                    artifacts: artifacts.clone(),
                    log_dir: log_dir.clone(),
                    on_event: Some(Arc::new(move |event| {
                        event_sink_clone(json!({
                            "projectId": project_id,
                            "sessionId": session_id,
                            "event": event,
                            "iterateStage": "qa",
                            "iterateLoop": current_loop
                        }));
                    })),
                    cancellation_token: Some(Arc::clone(&control.force_stop)),
                    max_turns: Some(30),
                    ..Default::default()
                },
            )
            .await?;

        if let Some(tokens) = qa_result.token_usage {
            total_session_tokens += tokens as i64;
        }

        let qa_report = qa_result.content;
        last_qa_report = Some(qa_report.clone());

        event_sink(json!({
            "projectId": project_id,
            "sessionId": session_id,
            "event": {
                "type": "iterate_stage_complete",
                "data": { "loop": current_loop, "stage": "qa", "output": qa_report }
            }
        }));

        // ---------------------------------------------------------------------
        // RECORD LOOP COMPLETION IN DATABASE
        // ---------------------------------------------------------------------
        let loop_card_data = json!({
            "isIterationLoop": true,
            "loopNumber": current_loop,
            "plan": plan_content,
            "implementation": implementation_summary,
            "qaReport": qa_report,
            "status": if control.wrap_up.load(Ordering::Relaxed) { "wrapped_up" } else { "completed" }
        });

        messages.push(ChatMessage {
            role: "assistant".to_string(),
            content: format!("### Iteration #{} Complete\n\n**Plan Objective**:\n{}\n\n**Implementation Summary**:\n{}\n\n**QA Report**:\n{}", current_loop, plan_content, implementation_summary, qa_report),
            data: Some(loop_card_data),
        });

        let history = serde_json::to_string(&messages)?;
        let title = initial_user_prompt.chars().take(48).collect::<String>();
        let _ = sqlx::query("UPDATE sessions SET title = ?, conversation_history = ?, token_usage = ? WHERE id = ? AND project_id = ?")
            .bind(&title)
            .bind(&history)
            .bind(total_session_tokens)
            .bind(session_id)
            .bind(project_id)
            .execute(pool)
            .await;

        event_sink(json!({
            "projectId": project_id,
            "sessionId": session_id,
            "event": {
                "type": "iterate_loop_complete",
                "data": {
                    "loop": current_loop,
                    "plan": plan_content,
                    "implementation": implementation_summary,
                    "qaReport": qa_report,
                    "tokenUsage": total_session_tokens
                }
            }
        }));

        // Stop if wrap up was requested or forced stop
        if control.wrap_up.load(Ordering::Relaxed) || control.force_stop.load(Ordering::Relaxed) {
            event_sink(json!({
                "projectId": project_id,
                "sessionId": session_id,
                "event": {
                    "type": "iterate_status",
                    "data": { "phase": "completed", "reason": "wrapped_up", "loop": current_loop }
                }
            }));
            break;
        }

        current_loop += 1;
    }

    let final_history = serde_json::to_string(&messages)?;
    Ok(Session {
        id: session_id,
        title: initial_user_prompt.chars().take(48).collect::<String>(),
        conversation_history: final_history,
        token_usage: total_session_tokens,
        compacted_summary: None,
    })
}
