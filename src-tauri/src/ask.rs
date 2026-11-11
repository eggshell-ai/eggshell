//! `ask_user` rendezvous — the transport half of the ask-user handshake.
//!
//! An agent tool cannot reach the UI on its own, so this module owns the
//! channel registry:
//!
//! 1. The tool calls [`AskBroker::ask`], which mints a question id, stores a
//!    `tokio::sync::oneshot` sender in a process-wide registry, and emits the
//!    question through the run's **event sink** — so it is streamed to the UI
//!    *and* persisted into the conversation history like every other event.
//! 2. The UI renders the card; answering calls the `answer_question` Tauri
//!    command, which calls [`answer`] and resolves the sender.
//! 3. The parked tool wakes up with the user's answer.
//!
//! Because tools are constructed per run and hold no application state, the
//! registry is a global — the same pattern `db::pool()` and `Logger::global()`
//! already use. Ids are unique per process, so concurrent questions from
//! different projects cannot collide.
//!
//! Cancellation is delivered through this channel rather than through
//! [`crate::llm::AgentOptions::cancellation_token`], because a tool cannot see
//! those options. `stop_chat` calls [`cancel_project`], which resolves every
//! pending question for that project with [`AskOutcome::Cancelled`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use tokio::sync::oneshot;

/// A question the agent wants the user to answer.
#[derive(Debug, Clone)]
pub struct AskRequest {
    pub id: String,
    pub project_id: i64,
    pub session_id: i64,
    pub question: String,
    pub options: Vec<String>,
}

/// How a pending question was resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AskOutcome {
    /// The user picked an option or typed an answer.
    Answered(String),
    /// The run was stopped before the user answered.
    Cancelled,
}

struct Pending {
    project_id: i64,
    sender: oneshot::Sender<AskOutcome>,
}

fn registry() -> &'static Mutex<HashMap<String, Pending>> {
    static PENDING: OnceLock<Mutex<HashMap<String, Pending>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock() -> std::sync::MutexGuard<'static, HashMap<String, Pending>> {
    registry().lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Hands questions to the UI for one agent run.
///
/// The run's event sink is captured here so the question travels the exact same
/// path as `tool_call`/`thought` events: streamed live **and** persisted into
/// `conversation_history`. Emitting directly from the tool would make the card
/// vanish on reload.
#[derive(Clone)]
pub struct AskBroker {
    sink: Arc<dyn Fn(Value) + Send + Sync>,
}

impl AskBroker {
    pub fn new(sink: Arc<dyn Fn(Value) + Send + Sync>) -> Self {
        Self { sink }
    }

    /// Emits the question and returns a receiver that resolves once the user
    /// answers it (or the run is stopped).
    ///
    /// The registry entry is created *before* the emit so an instantaneous
    /// answer cannot race ahead of it.
    pub fn ask(&self, request: AskRequest) -> oneshot::Receiver<AskOutcome> {
        let receiver = register(&request);
        (self.sink)(question_event(&request, true));
        receiver
    }

    /// Emits the question without waiting for an answer.
    ///
    /// Used by unattended runs (Max mode), where blocking would hang the
    /// sequence forever. The question is still displayed so the user can answer
    /// later as a normal chat message.
    pub fn announce(&self, request: &AskRequest) {
        (self.sink)(question_event(request, false));
    }
}

/// Builds the `ask_user` agent event.
///
/// `awaiting` tells the UI whether the run is parked on this question. It is
/// `false` for unattended runs, where the card is informational only.
fn question_event(request: &AskRequest, awaiting: bool) -> Value {
    json!({
        "type": "ask_user",
        "data": {
            "questionId": request.id,
            "question": request.question,
            "options": request.options,
            "awaitingAnswer": awaiting,
            "projectId": request.project_id,
            "sessionId": request.session_id,
        }
    })
}

/// Registers a pending question and returns the channel the tool awaits.
pub fn register(request: &AskRequest) -> oneshot::Receiver<AskOutcome> {
    let (sender, receiver) = oneshot::channel();
    lock().insert(
        request.id.clone(),
        Pending {
            project_id: request.project_id,
            sender,
        },
    );
    receiver
}

/// Builds a question id. Not a cryptographic UUID: it only has to be unique
/// within this process.
pub fn new_question_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    format!("q-{nanos}-{}", std::process::id())
}

/// Resolves a pending question with the user's answer.
///
/// Returns an error when the id is unknown — usually because the run already
/// finished, was stopped, or the app was restarted.
pub fn answer(question_id: &str, answer: String) -> Result<(), String> {
    let pending = lock()
        .remove(question_id)
        .ok_or_else(|| "That question is no longer waiting for an answer.".to_string())?;
    pending
        .sender
        .send(AskOutcome::Answered(answer))
        .map_err(|_| "The agent stopped waiting for this answer.".to_string())
}

/// Cancels every pending question belonging to a project.
///
/// Called by `stop_chat` so a blocked tool unwinds instead of holding the run
/// open. Returns how many questions were cancelled.
pub fn cancel_project(project_id: i64) -> usize {
    let cancelled: Vec<Pending> = {
        let mut pending = lock();
        let ids: Vec<String> = pending
            .iter()
            .filter(|(_, entry)| entry.project_id == project_id)
            .map(|(id, _)| id.clone())
            .collect();
        ids.into_iter()
            .filter_map(|id| pending.remove(&id))
            .collect()
    };

    let count = cancelled.len();
    for entry in cancelled {
        let _ = entry.sender.send(AskOutcome::Cancelled);
    }
    count
}

/// Serialises an outcome for logging. Kept here so the tool does not duplicate
/// the mapping.
pub fn outcome_label(outcome: &AskOutcome) -> Value {
    match outcome {
        AskOutcome::Answered(text) => json!({ "answered": true, "answer": text }),
        AskOutcome::Cancelled => json!({ "answered": false, "cancelled": true }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(id: &str, project_id: i64) -> oneshot::Receiver<AskOutcome> {
        register(&AskRequest {
            id: id.to_string(),
            project_id,
            session_id: 1,
            question: "Pick one".to_string(),
            options: Vec::new(),
        })
    }

    #[test]
    fn answering_resolves_the_waiting_receiver() {
        let receiver = register("q-answer", 7);
        answer("q-answer", "Option B".to_string()).expect("answer resolves");
        assert_eq!(
            receiver.blocking_recv().expect("outcome"),
            AskOutcome::Answered("Option B".to_string())
        );
    }

    #[test]
    fn answering_an_unknown_question_is_an_error() {
        let error = answer("q-missing", "x".to_string()).expect_err("unknown id");
        assert!(error.contains("no longer waiting"));
    }

    #[test]
    fn a_question_can_only_be_answered_once() {
        let _receiver = register("q-once", 7);
        answer("q-once", "first".to_string()).expect("first answer wins");
        assert!(answer("q-once", "second".to_string()).is_err());
    }

    #[test]
    fn cancelling_a_project_resolves_only_its_questions() {
        let mine = register("q-cancel-mine", 11);
        let other = register("q-cancel-other", 12);

        assert_eq!(cancel_project(11), 1);
        assert_eq!(mine.blocking_recv().expect("outcome"), AskOutcome::Cancelled);

        // The other project's question is untouched, so answering still works.
        answer("q-cancel-other", "kept".to_string()).expect("other answer");
        assert_eq!(
            other.blocking_recv().expect("outcome"),
            AskOutcome::Answered("kept".to_string())
        );
    }

    #[test]
    fn cancelling_a_project_without_questions_is_a_no_op() {
        assert_eq!(cancel_project(9999), 0);
    }

    #[test]
    fn question_ids_are_usable_as_registry_keys() {
        let first = new_question_id();
        let second = new_question_id();
        assert!(first.starts_with("q-"));
        assert_ne!(first, second);
    }
}
