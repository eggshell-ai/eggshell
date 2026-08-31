use super::{AgentOptions, LLMMessage, LLMMessageRole, LLMService, LlmResult, Skill, Tool};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_MAX_TURNS: u32 = 10;

/// The final output of an agent run.
#[derive(Debug, Clone, Serialize)]
pub struct AgentRunResult {
    pub content: String,
    pub tool_calls: Vec<Value>,
}

/// Executes an LLM conversation until it produces a response without tool calls.
///
/// The conversation is represented entirely by `LLMMessage`; no separate agent
/// message type is used.
pub struct AgentService {
    llm_service: Arc<dyn LLMService>,
    skills: Vec<Box<dyn Skill>>,
}

impl AgentService {
    pub fn new(llm_service: Arc<dyn LLMService>) -> Self {
        Self::with_skills(llm_service, super::default_skills())
    }

    pub fn with_skills(llm_service: Arc<dyn LLMService>, skills: Vec<Box<dyn Skill>>) -> Self {
        Self {
            llm_service,
            skills,
        }
    }

    /// Runs an existing conversation. Include system and user messages in
    /// `messages`; available skills are appended to the system message.
    pub async fn run_agent(
        &self,
        mut messages: Vec<LLMMessage>,
        tools: Vec<Box<dyn Tool>>,
        options: AgentOptions,
    ) -> LlmResult<AgentRunResult> {
        self.attach_skills_summary(&mut messages);

        let max_turns = options.max_turns.unwrap_or(DEFAULT_MAX_TURNS);
        let context = options.context.clone().unwrap_or_default();
        let start_ms = now_millis()?;
        let conversation_id = format!("{}-{}", start_ms, std::process::id());
        let mut turn = 0;
        let mut all_tool_calls = Vec::new();
        let mut log = Vec::new();
        let mut final_content = String::new();

        while turn < max_turns {
            turn += 1;
            let response = self
                .llm_service
                .execute_prompt_with_tools(&messages, &tools, Some(&context))
                .await?;
            let content = response
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let tool_calls = response
                .get("tool_calls")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();

            final_content = content.clone();
            emit(
                &options,
                "thought",
                json!({ "content": content, "turn": turn }),
            );
            log.push(json!({ "timestamp": now_millis()?, "turn": turn, "type": "thought", "content": content }));
            messages.push(LLMMessage {
                role: LLMMessageRole::Assistant,
                content,
                tool_call_id: None,
                tool_name: None,
                tool_args: None,
                tool_calls: Some(tool_calls.clone()),
            });

            if tool_calls.is_empty() {
                break;
            }
            all_tool_calls.extend(tool_calls.clone());

            for tool_call in tool_calls {
                let (name, args) = tool_call_parts(&tool_call);
                let Some(tool) = tools.iter().find(|tool| tool.name() == name) else {
                    log.push(json!({ "timestamp": now_millis()?, "turn": turn, "type": "tool_error", "toolName": name, "error": "Tool not found" }));
                    continue;
                };

                // The project selected by the caller is authoritative. Tool
                // schemas expose projectPath for compatibility, but relying on
                // the model to supply it allows filesystem writes to fall back
                // to a path relative to the app's current working directory.
                let tool_args = with_project_path(args.clone(), options.project_path.as_deref());
                emit(
                    &options,
                    "tool_call",
                    json!({ "name": name, "arguments": tool_args }),
                );
                log.push(json!({ "timestamp": now_millis()?, "turn": turn, "type": "tool_call", "toolName": name, "toolArgs": tool_args }));
                let call_id = tool_call
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                match tool.execute(tool_args.clone()).await {
                    Ok(result) => {
                        emit(
                            &options,
                            "tool_result",
                            json!({ "name": name, "result": result }),
                        );
                        log.push(json!({ "timestamp": now_millis()?, "turn": turn, "type": "tool_result", "toolName": name, "toolResult": result }));
                        messages.push(tool_message(call_id, name, tool_args, result));
                    }
                    Err(error) => {
                        let error = error.to_string();
                        let result = json!({ "error": error });
                        emit(
                            &options,
                            "tool_result",
                            json!({ "name": name, "result": result }),
                        );
                        log.push(json!({ "timestamp": now_millis()?, "turn": turn, "type": "tool_error", "toolName": name, "error": result["error"] }));
                        messages.push(tool_message(call_id, name, tool_args, result));
                    }
                }
            }
        }

        let result = AgentRunResult {
            content: final_content,
            tool_calls: all_tool_calls,
        };
        emit(
            &options,
            "complete",
            json!({ "content": result.content, "tool_calls": result.tool_calls }),
        );
        if options.log_conversation {
            self.write_conversation_log(
                &conversation_id,
                &log,
                &messages,
                &context,
                turn,
                max_turns,
                &result,
                options.log_dir.as_deref(),
            )?;
        }
        Ok(result)
    }

    fn attach_skills_summary(&self, messages: &mut Vec<LLMMessage>) {
        if self.skills.is_empty() {
            return;
        }
        let mut summary = String::from("\n\n=== AVAILABLE SKILLS ===\nYou have access to the following skills that teach you how to perform specific tasks:\n\n");
        for skill in &self.skills {
            summary.push_str(&format!(
                "**{}**\nDescription: {}\n",
                skill.name(),
                skill.description()
            ));
            if let Some(category) = skill.category() {
                summary.push_str(&format!("Category: {category}\n"));
            }
            if !skill.tags().is_empty() {
                summary.push_str(&format!("Tags: {}\n", skill.tags().join(", ")));
            }
            summary.push('\n');
        }
        summary.push_str(
            "To use a skill, call the load_skill tool with the skill name.\n=== END SKILLS ===\n",
        );
        if let Some(system) = messages
            .iter_mut()
            .find(|message| matches!(message.role, LLMMessageRole::System))
        {
            system.content.push_str(&summary);
        } else {
            messages.insert(
                0,
                LLMMessage {
                    role: LLMMessageRole::System,
                    content: summary,
                    tool_call_id: None,
                    tool_name: None,
                    tool_args: None,
                    tool_calls: None,
                },
            );
        }
    }

    fn write_conversation_log(
        &self,
        id: &str,
        flow: &[Value],
        messages: &[LLMMessage],
        context: &Map<String, Value>,
        turns: u32,
        max_turns: u32,
        result: &AgentRunResult,
        configured_dir: Option<&str>,
    ) -> LlmResult<()> {
        let directory = configured_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("logs/agent-conversations"));
        fs::create_dir_all(&directory)?;
        let data = json!({ "conversationId": id, "metadata": { "maxTurns": max_turns, "actualTurns": turns, "context": context, "finalContent": result.content, "allToolCalls": result.tool_calls }, "messages": messages, "conversationLog": flow });
        fs::write(
            directory.join(format!("{id}.json")),
            serde_json::to_string_pretty(&data)?,
        )?;
        fs::write(
            directory.join(format!("{id}.log")),
            serde_json::to_string_pretty(&data)?,
        )?;
        Ok(())
    }
}

fn tool_call_parts(call: &Value) -> (&str, Value) {
    let function = call.get("function").unwrap_or(call);
    let name = function
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let raw_args = function
        .get("arguments")
        .or_else(|| call.get("arguments"))
        .cloned()
        .unwrap_or(Value::Null);
    let args = raw_args
        .as_str()
        .and_then(|text| serde_json::from_str(text).ok())
        .unwrap_or(raw_args);
    (name, args)
}

fn tool_message(id: Option<String>, name: &str, args: Value, result: Value) -> LLMMessage {
    LLMMessage {
        role: LLMMessageRole::Tool,
        content: if let Some(text) = result.as_str() {
            text.to_owned()
        } else {
            result.to_string()
        },
        tool_call_id: id,
        tool_name: Some(name.to_owned()),
        tool_args: Some(args),
        tool_calls: None,
    }
}

fn with_project_path(args: Value, project_path: Option<&str>) -> Value {
    let Some(project_path) = project_path.filter(|path| !path.trim().is_empty()) else {
        return args;
    };
    let mut args = match args {
        Value::Object(args) => args,
        args => return args,
    };
    args.insert(
        "projectPath".to_string(),
        Value::String(project_path.to_owned()),
    );
    Value::Object(args)
}

fn emit(options: &AgentOptions, kind: &str, data: Value) {
    if let Some(callback) = &options.on_event {
        callback(json!({ "type": kind, "data": data }));
    }
}

fn now_millis() -> LlmResult<u128> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| io::Error::new(io::ErrorKind::Other, error))?
        .as_millis())
}
