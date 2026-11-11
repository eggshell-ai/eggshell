//! `ask_user` — lets the agent ask the user a question and wait for the answer.
//!
//! The tool is a thin wrapper around [`crate::ask`]: it validates the arguments,
//! hands the question to the broker, and turns the outcome into a tool result.
//!
//! Two behaviours share one tool:
//!
//! * **Interactive runs** (implement, plan) block until the user answers. The
//!   answer becomes the tool result, so the agent continues in the same turn
//!   with real information instead of guessing.
//! * **Unattended runs** (Max mode) never block. The question is still shown in
//!   the UI, but the tool returns immediately and tells the model to proceed
//!   with its best judgment — otherwise the sequence would hang forever.

use async_trait::async_trait;
use serde_json::{json, Map, Value};

use crate::ask::{self, AskBroker, AskOutcome, AskRequest};
use crate::llm::{LlmResult, Tool};

/// The most choices a single question may offer. More than this is a decision
/// tree, not a question, and it does not fit the card comfortably.
const MAX_OPTIONS: usize = 8;

pub struct AskUserTool {
    broker: Option<AskBroker>,
    project_id: i64,
    session_id: i64,
    /// `false` for unattended runs, which display the question but never wait.
    interactive: bool,
    parameters: Map<String, Value>,
}

impl AskUserTool {
    pub fn new(
        broker: Option<AskBroker>,
        project_id: i64,
        session_id: i64,
        interactive: bool,
    ) -> Self {
        Self {
            broker,
            project_id,
            session_id,
            interactive,
            parameters: json!({
                "type": "object",
                "properties": {
                    "question": {
                        "type": "string",
                        "description": "The question to show the user. Ask one focused question and explain briefly why you need the answer."
                    },
                    "options": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional short choices to offer as buttons. The user can always type a free-form answer instead. Prefer 2-5 options."
                    }
                },
                "required": ["question"]
            })
            .as_object()
            .expect("ask_user parameters must be an object")
            .clone(),
        }
    }

    /// Builds the request from validated arguments.
    fn request(&self, question: String, options: Vec<String>) -> AskRequest {
        AskRequest {
            id: ask::new_question_id(),
            project_id: self.project_id,
            session_id: self.session_id,
            question,
            options,
        }
    }
}

#[async_trait]
impl Tool for AskUserTool {
    fn name(&self) -> &str {
        "ask_user"
    }

    fn description(&self) -> &str {
        "Asks the user a question and waits for their answer. Use it when a decision or a missing detail would change what you build — never for something you can determine from the project. You may offer a few options as buttons; the user can always type a free-form answer instead. Ask one question at a time, and do not use it to confirm work you are already sure about."
    }

    fn parameters(&self) -> &Map<String, Value> {
        &self.parameters
    }

    async fn execute(&self, args: Value) -> LlmResult<Value> {
        let question = args
            .get("question")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|question| !question.is_empty())
            .ok_or_else(|| "question is required and must be a non-empty string".to_string())?
            .to_string();

        let options = parse_options(&args)?;

        let Some(broker) = self.broker.as_ref() else {
            return Ok(json!({
                "answered": false,
                "reason": "The user is not available right now. Proceed using best judgment and state the assumption you made."
            }));
        };

        let request = self.request(question, options);
        let question_id = request.id.clone();

        if !self.interactive {
            broker.announce(&request);
            return Ok(json!({
                "questionId": question_id,
                "answered": false,
                "reason": "This run is unattended, so the question was queued for the user but could not be answered now. Proceed using best judgment and state the assumption you made."
            }));
        }

        let outcome = broker.ask(request).await.map_err(|_| {
            "The question could not be delivered because the run is shutting down.".to_string()
        })?;

        match outcome {
            AskOutcome::Answered(answer) => Ok(json!({
                "questionId": question_id,
                "answered": true,
                "answer": answer
            })),
            AskOutcome::Cancelled => {
                Err("The user stopped the run before answering the question.".into())
            }
        }
    }
}

/// Validates the optional `options` array: strings, trimmed, de-duplicated, and
/// capped. Blank entries are dropped rather than rejected, because a stray
/// empty string should not cost a whole turn.
fn parse_options(args: &Value) -> LlmResult<Vec<String>> {
    let Some(raw) = args.get("options") else {
        return Ok(Vec::new());
    };
    let Some(items) = raw.as_array() else {
        return Err("options must be an array of strings".to_string().into());
    };

    let mut options: Vec<String> = Vec::new();
    for item in items {
        let value = item
            .as_str()
            .ok_or_else(|| "options must be an array of strings".to_string())?
            .trim();
        if value.is_empty() || options.iter().any(|existing| existing == value) {
            continue;
        }
        options.push(value.to_string());
        if options.len() == MAX_OPTIONS {
            break;
        }
    }
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_are_trimmed_deduplicated_and_capped() {
        let options = parse_options(&json!({
            "options": ["  A ", "A", "", "B", "C", "D", "E", "F", "G", "H"]
        }))
        .expect("valid options");
        assert_eq!(options, vec!["A", "B", "C", "D", "E", "F", "G", "H"]);
    }

    #[test]
    fn options_are_optional_and_may_be_empty() {
        assert!(parse_options(&json!({})).unwrap().is_empty());
        assert!(parse_options(&json!({ "options": [] })).unwrap().is_empty());
    }

    #[test]
    fn a_deduplicated_list_shorter_than_the_cap_is_kept() {
        let options = parse_options(&json!({ "options": ["Yes", "No"] })).expect("valid");
        assert_eq!(options, vec!["Yes", "No"]);
    }

    #[test]
    fn non_string_options_are_rejected() {
        assert!(parse_options(&json!({ "options": [1, 2] })).is_err());
        assert!(parse_options(&json!({ "options": "Yes" })).is_err());
    }

    #[tokio::test]
    async fn an_unattended_run_never_waits_for_an_answer() {
        // No broker at all stands in for "no UI attached": the tool must still
        // return immediately with an instruction to proceed.
        let tool = AskUserTool::new(None, 1, 2, true);
        let result = tool
            .execute(json!({ "question": "Which currency?", "options": ["USD", "EUR"] }))
            .await
            .expect("tool succeeds");

        assert_eq!(result["answered"], false);
        assert!(result["reason"].as_str().unwrap().contains("best judgment"));
    }

    #[tokio::test]
    async fn a_blank_question_is_rejected() {
        let tool = AskUserTool::new(None, 1, 2, true);
        let error = tool
            .execute(json!({ "question": "   " }))
            .await
            .expect_err("blank question rejected");
        assert!(error.to_string().contains("question is required"));
    }

    #[tokio::test]
    async fn a_question_without_choices_is_allowed() {
        let tool = AskUserTool::new(None, 1, 2, false);
        let result = tool
            .execute(json!({ "question": "What should the report cover?" }))
            .await
            .expect("tool succeeds");
        assert_eq!(result["answered"], false);
    }
}
