use super::{AgentOptions, LLMMessage, LLMMessageRole, LLMService, LlmResult, Skill, Tool};
use crate::logger::Logger;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_MAX_TURNS: u32 = 10;

/// How many times `sync_schema` may rename a field on the same resource within
/// one run before the run is treated as stalled.
///
/// A rename drops and recreates a database column, so model-driven rename
/// churn is never productive: if the name were already known, one call would
/// have been enough. The sales-system incident needed three renames to be
/// recognised as a loop. The threshold leaves room for one deliberate rename
/// followed by a correction.
const MAX_SCHEMA_RENAMES_PER_RESOURCE: u32 = 3;

/// A file the user attached to a message. Only its name travels into the
/// prompt; the contents are deliberately never sent to the upstream provider.
#[derive(Debug, Clone)]
pub struct AgentArtifact {
    pub name: String,
}

/// The final output of an agent run.
#[derive(Debug, Clone, Serialize)]
pub struct AgentRunResult {
    pub content: String,
    pub tool_calls: Vec<Value>,
    pub token_usage: Option<u64>,
    #[serde(default)]
    pub hit_max_turns: bool,
    /// The turn budget the run was given. Reported alongside `hit_max_turns` so
    /// callers can explain the stop without duplicating the default.
    #[serde(default)]
    pub max_turns: u32,
    /// Set when the model repeated a tool call that had already failed twice
    /// with identical arguments. The run stops instead of spending the rest of
    /// the turn budget on a call that cannot succeed.
    #[serde(default)]
    pub stalled: bool,
}

/// Executes an LLM conversation until it produces a response without tool calls.
///
/// The conversation is represented entirely by `LLMMessage`; no separate agent
/// message type is used.
pub struct AgentService {
    llm_service: Arc<dyn LLMService>,
    skills: Vec<Box<dyn Skill>>,
    /// Central logger; agent activity is mirrored into it so reports can quote it.
    logger: Logger,
}

impl AgentService {
    pub fn new(llm_service: Arc<dyn LLMService>) -> Self {
        Self::with_skills(llm_service, super::default_skills())
    }

    pub fn with_skills(llm_service: Arc<dyn LLMService>, skills: Vec<Box<dyn Skill>>) -> Self {
        Self {
            llm_service,
            skills,
            logger: Logger::new(),
        }
    }

    /// Shares the central logger so agent entries land in the same log the rest
    /// of the application reports from.
    pub fn with_logger(mut self, logger: Logger) -> Self {
        self.logger = logger;
        self
    }

    pub fn logger(&self) -> &Logger {
        &self.logger
    }

    /// Runs an existing conversation. Include system and user messages in
    /// `messages`; available skills are appended to the system message.
    pub async fn run_agent(
        &self,
        mut messages: Vec<LLMMessage>,
        tools: Vec<Box<dyn Tool>>,
        options: AgentOptions,
    ) -> LlmResult<AgentRunResult> {
        let skills = options.skills.as_deref().unwrap_or(&self.skills);
        // Project skills are the ones `enable_module` installed. They are read
        // per run because a module can be enabled while the app is running, and
        // its skill must be visible to the very next turn. A bundled skill of the
        // same name wins: the application's own instructions are authoritative.
        //
        // `AgentOptions::skills` is an explicit opt-out as well as an override:
        // max mode passes `Some(vec![])` so its planner prompt carries no skill
        // catalogue at all, and a module must not quietly reintroduce one.
        let installed = if options.skills.is_none() {
            super::project_skills(options.project_path.as_deref())
        } else {
            Vec::new()
        };
        let project_skills: Vec<&dyn Skill> = installed
            .iter()
            .map(|skill| skill.as_ref())
            .filter(|skill| !skills.iter().any(|bundled| bundled.name() == skill.name()))
            .collect();
        self.attach_skills_summary(&mut messages, skills, &project_skills);
        self.attach_artifacts(&mut messages, &options.artifacts);

        // The prompts themselves are sensitive: they only travel into the
        // diagnostics when the user explicitly opts in.
        for message in &messages {
            self.logger.info(
                format!("agent prompt [{}] {}", role_name(&message.role), message.content),
                true,
            );
        }

        let max_turns = options.max_turns.unwrap_or(DEFAULT_MAX_TURNS);
        let context = options.context.clone().unwrap_or_default();
        let start_ms = now_millis()?;
        let conversation_id = format!("{}-{}", start_ms, std::process::id());
        let mut turn = 0;
        let mut all_tool_calls = Vec::new();
        let mut log = Vec::new();
        let mut final_content = String::new();
        let mut latest_token_usage: Option<u64> = None;
        let mut hit_max_turns = false;
        // Tracks a failing call so a byte-identical retry can be intercepted
        // instead of being executed again. See `fingerprint_tool_call`.
        let mut failed_call: Option<(String, u32)> = None;
        // Counts mutating tool calls per "target" (tool + resource + field), so
        // a run that keeps editing the same thing with slightly different
        // arguments (e.g. renaming a field A -> B -> A) is stopped. Each such
        // call succeeds, so `failed_call` never fires. See
        // `mutation_target_key`.
        let mut mutation_counts: std::collections::HashMap<String, u32> =
            std::collections::HashMap::new();
        let mut stalled = false;
        let cancel_token = options.cancellation_token.clone();

        while turn < max_turns {
            if cancel_token.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
                self.logger.info("agent run interrupted by user".to_string(), false);
                break;
            }
            turn += 1;

            let stream_cb: Option<crate::llm::StreamCallback> = options.on_event.as_ref().map(|on_event| {
                let on_event = Arc::clone(on_event);
                Arc::new(move |chunk: crate::llm::StreamChunk| {
                    match chunk {
                        crate::llm::StreamChunk::ThoughtDelta(delta) => {
                            on_event(json!({ "type": "thought_delta", "data": { "delta": delta, "turn": turn } }));
                        }
                        crate::llm::StreamChunk::ContentDelta(delta) => {
                            on_event(json!({ "type": "content_delta", "data": { "delta": delta, "turn": turn } }));
                        }
                    }
                }) as crate::llm::StreamCallback
            });

            let response = match self
                .llm_service
                .execute_prompt_with_tools_cancellable(&messages, &tools, Some(&context), cancel_token.clone(), stream_cb)
                .await
            {
                Ok(resp) => resp,
                Err(err) => {
                    if cancel_token.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
                        self.logger.info("agent LLM call interrupted by user".to_string(), false);
                        break;
                    }
                    return Err(err);
                }
            };
            let content = response
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let thought = response
                .get("thought")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let tool_calls = response
                .get("tool_calls")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let usage = response.get("usage").cloned();
            if let Some(u) = &usage {
                if let Some(total) = u.get("total_tokens").and_then(Value::as_u64) {
                    latest_token_usage = Some(total);
                    emit(
                        &options,
                        "token_usage",
                        json!({
                            "turn": turn,
                            "prompt_tokens": u.get("prompt_tokens").and_then(Value::as_u64).unwrap_or(0),
                            "completion_tokens": u.get("completion_tokens").and_then(Value::as_u64).unwrap_or(0),
                            "total_tokens": total,
                        }),
                    );
                }
            }

            final_content = content.clone();
            if !thought.is_empty() {
                self.logger.info(format!("agent turn {turn} reasoning: {thought}"), true);
                emit(
                    &options,
                    "thought",
                    json!({ "content": thought, "turn": turn }),
                );
                log.push(json!({ "timestamp": now_millis()?, "turn": turn, "type": "thought", "content": thought }));
            } else if !tool_calls.is_empty() && !content.is_empty() {
                self.logger.info(format!("agent turn {turn} thought: {content}"), true);
                emit(
                    &options,
                    "thought",
                    json!({ "content": content, "turn": turn }),
                );
                log.push(json!({ "timestamp": now_millis()?, "turn": turn, "type": "thought", "content": content }));
            }
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
            if turn >= max_turns {
                self.logger.warning(format!("agent reached maximum turns ({max_turns})"), true);
                hit_max_turns = true;
                break;
            }
            all_tool_calls.extend(tool_calls.clone());

            let mut cancelled_during_tools = false;
            for tool_call in tool_calls {
                if cancel_token.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
                    cancelled_during_tools = true;
                    break;
                }
                let (name, args) = tool_call_parts(&tool_call);
                let Some(tool) = tools.iter().find(|tool| tool.name() == name) else {
                    log.push(json!({ "timestamp": now_millis()?, "turn": turn, "type": "tool_error", "toolName": name, "error": "Tool not found" }));
                    continue;
                };

                // The project selected by the caller is authoritative. Tool
                // schemas expose projectPath for compatibility, but relying on
                // the model to supply it allows filesystem writes to fall back
                // to a path relative to the app's current working directory.
                let call_id = tool_call
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                let tool_args = with_project_path(args.clone(), options.project_path.as_deref());

                // A byte-identical retry of a call that already failed cannot
                // succeed. Intercept it instead of executing it again: on the
                // second attempt, answer with a corrective tool result; on the
                // third, stop the run so the remaining turn budget is not
                // spent looping (see the sales-system computed-field incident).
                let fingerprint = fingerprint_tool_call(name, &tool_args);
                let repeat_count = match &failed_call {
                    Some((failed, count)) if *failed == fingerprint => *count,
                    _ => 0,
                };
                if repeat_count >= 2 {
                    self.logger.warning(
                        format!("agent stalled: tool {name} repeated an identical failing call {repeat_count} times"),
                        true,
                    );
                    log.push(json!({ "timestamp": now_millis()?, "turn": turn, "type": "tool_repeat", "toolName": name, "attempts": repeat_count, "stalled": true }));
                    stalled = true;
                    break;
                }
                if repeat_count > 0 {
                    let payload = json!({
                        "success": false,
                        "error": "This is an identical repeat of a tool call that already failed. It cannot succeed and was not executed again. Change the arguments before retrying - for `sync_schema` that sometimes means putting the new properties in `changes` or `changes.type` instead of `rename`.",
                        "repeatedCall": { "name": name, "arguments": tool_args },
                        "attempt": repeat_count + 1,
                    });
                    self.logger.warning(
                        format!("agent turn {turn} blocked identical retry of {name} (attempt {})", repeat_count + 1),
                        true,
                    );
                    emit(
                        &options,
                        "tool_result",
                        json!({ "id": call_id, "name": name, "result": payload }),
                    );
                    log.push(json!({ "timestamp": now_millis()?, "turn": turn, "type": "tool_repeat", "toolName": name, "attempts": repeat_count + 1, "stalled": false }));
                    messages.push(tool_message(call_id, name, tool_args, payload));
                    failed_call = Some((fingerprint, repeat_count + 1));
                    continue;
                }

                // Guard against oscillation: repeated edits to the same field
                // whose arguments only differ in `rename` all succeed, so the
                // identical-call guard above never fires. The sales-system
                // incident renamed saleId -> saleRef -> saleId -> sale_id_tmp
                // across four turns and would have continued.
                if let Some(key) = schema_rename_target(name, &tool_args) {
                    let count = mutation_counts.entry(key).or_insert(0);
                    *count += 1;
                    if *count > MAX_SCHEMA_RENAMES_PER_RESOURCE {
                        self.logger.warning(
                            format!(
                                "agent stalled: {name} renamed a field on the same resource {} times",
                                *count
                            ),
                            true,
                        );
                        log.push(json!({ "timestamp": now_millis()?, "turn": turn, "type": "tool_repeat", "toolName": name, "attempts": *count, "stalled": true }));
                        stalled = true;
                        break;
                    }
                }

                self.logger.info(
                    format!("agent turn {turn} tool call {name}: {tool_args}"),
                    true,
                );
                emit(
                    &options,
                    "tool_call",
                    json!({ "id": call_id, "name": name, "arguments": tool_args }),
                );
                log.push(json!({ "timestamp": now_millis()?, "turn": turn, "type": "tool_call", "toolName": name, "toolArgs": tool_args }));
                match tool.execute(tool_args.clone()).await {
                    Ok(result) => {
                        self.logger.info(
                            format!("agent turn {turn} tool result {name}: {result}"),
                            true,
                        );
                        emit(
                            &options,
                            "tool_result",
                            json!({ "id": call_id, "name": name, "result": result }),
                        );
                        log.push(json!({ "timestamp": now_millis()?, "turn": turn, "type": "tool_result", "toolName": name, "toolResult": result }));
                        messages.push(tool_message(call_id, name, tool_args, result));
                        // Only clear the tracker if the call that just succeeded
                        // is the one being tracked; an unrelated success in the
                        // same turn must not mask a still-failing call.
                        if failed_call
                            .as_ref()
                            .is_some_and(|(failed, _)| *failed == fingerprint)
                        {
                            failed_call = None;
                        }
                    }
                    Err(error) => {
                        let error = error.to_string();
                        self.logger
                            .error(format!("agent turn {turn} tool {name} failed: {error}"), true);
                        failed_call = Some((fingerprint, 1));
                        let result = json!({ "error": error });
                        emit(
                            &options,
                            "tool_result",
                            json!({ "id": call_id, "name": name, "result": result }),
                        );
                        log.push(json!({ "timestamp": now_millis()?, "turn": turn, "type": "tool_error", "toolName": name, "error": result["error"] }));
                        messages.push(tool_message(call_id, name, tool_args, result));
                    }
                }
            }
            if cancelled_during_tools {
                self.logger.info("agent tool loop interrupted by user".to_string(), false);
                break;
            }
            if stalled {
                break;
            }
        }

        let cancelled = cancel_token.as_ref().is_some_and(|c| c.load(Ordering::Relaxed));
        if hit_max_turns {
            // The final turn asked for tools that were never executed, so any
            // text it produced is a truncated preamble rather than an answer.
            // Replace it so the UI does not look like the agent trailed off.
            final_content = max_turns_notice(max_turns);
        } else if stalled {
            final_content =
                "Stopped: the same tool call kept failing with identical arguments. Change the approach or arguments before retrying.".to_string();
        } else if final_content.trim().is_empty() {
            if cancelled {
                final_content = "Response stopped by user.".to_string();
            } else if !all_tool_calls.is_empty() {
                final_content = "Completed requested actions.".to_string();
            }
        }

        let result = AgentRunResult {
            content: final_content,
            tool_calls: all_tool_calls,
            token_usage: latest_token_usage,
            hit_max_turns,
            max_turns,
            stalled,
        };
        emit(
            &options,
            "complete",
            json!({
                "content": result.content,
                "tool_calls": result.tool_calls,
                "token_usage": result.token_usage,
                "hit_max_turns": result.hit_max_turns,
                "max_turns": result.max_turns,
                "stalled": result.stalled,
            }),
        );
        if options.log_conversation {
            if let Err(error) = self.write_conversation_log(
                &conversation_id,
                &log,
                &messages,
                &context,
                turn,
                max_turns,
                &result,
                options.log_dir.as_deref(),
            ) {
                self.logger.error(
                    format!("Failed to write agent conversation log: {error}"),
                    false,
                );
            }
        }
        Ok(result)
    }

    /// Appends an artifacts section naming the files the user attached to
    /// this run. Only the names are added to the prompt — the files themselves
    /// are never uploaded to the upstream provider, so the model is told to ask
    /// for relevant excerpts (or read the file with a tool if it lives in the
    /// project) when the contents matter.
    fn attach_artifacts(&self, messages: &mut Vec<LLMMessage>, artifacts: &[AgentArtifact]) {
        if artifacts.is_empty() {
            return;
        }
        let mut section = String::from(
            "\n\n=== ATTACHED FILES ===\nThe user attached the following files to their message. Only the file names are listed; the contents were not uploaded:\n\n",
        );
        for artifact in artifacts {
            section.push_str(&format!("- {}\n", artifact.name));
        }
        section.push_str(
            "If the contents are relevant, ask the user to paste the relevant part, or use a tool to read the file if it exists in the project.\n=== END ATTACHED FILES ===\n",
        );
        match messages
            .iter_mut()
            .rev()
            .find(|message| matches!(message.role, LLMMessageRole::User))
        {
            Some(user) => user.content.push_str(&section),
            None => {
                if let Some(system) = messages
                    .iter_mut()
                    .find(|message| matches!(message.role, LLMMessageRole::System))
                {
                    system.content.push_str(&section);
                }
            }
        }
    }

    /// Appends the catalogue of `skills` to the conversation's system message.
    ///
    /// `project_skills` are the skills installed into the project by modules and
    /// are listed after the bundled ones. Empty slices append nothing, which lets
    /// a mode opt out of the skills section entirely through
    /// [`AgentOptions::skills`].
    fn attach_skills_summary(
        &self,
        messages: &mut Vec<LLMMessage>,
        skills: &[Box<dyn Skill>],
        project_skills: &[&dyn Skill],
    ) {
        let summary = skills_summary(skills, project_skills);
        if summary.is_empty() {
            return;
        }
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

fn role_name(role: &LLMMessageRole) -> &'static str {
    match role {
        LLMMessageRole::System => "system",
        LLMMessageRole::User => "user",
        LLMMessageRole::Assistant => "assistant",
        LLMMessageRole::Tool => "tool",
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

/// Stable identity for a tool call, used to detect an identical retry.
///
/// `serde_json::Map` is a `BTreeMap` in this build (the crate does not enable
/// `preserve_order`), so object keys serialize in a deterministic order and two
/// calls that differ only in key order still produce the same fingerprint.
fn fingerprint_tool_call(name: &str, args: &Value) -> String {
    format!("{name}:{}", serde_json::to_string(args).unwrap_or_default())
}

/// Returns a counter key when `name`/`args` describe a schema rename, so the
/// caller can count renames per resource across the run.
///
/// Only destructive field renames are counted, not ordinary edits: a run may
/// legitimately touch one field many times (labels, flags, then a type), but
/// renaming the same resource's fields over and over means the model is
/// thrashing. The key is `sync_schema:<resource>`.
fn schema_rename_target(name: &str, args: &Value) -> Option<String> {
    if name != "sync_schema" {
        return None;
    }
    let operations = args.get("operations")?.as_array()?;
    let mut resources: Vec<String> = Vec::new();
    for operation in operations {
        let object = operation.as_object()?;
        if object.get("action").and_then(Value::as_str) != Some("updateField") {
            continue;
        }
        let Some(field) = object.get("field").and_then(Value::as_str) else {
            continue;
        };
        let Some(rename) = object.get("rename").and_then(Value::as_str) else {
            continue;
        };
        // A `rename` equal to the current name is rejected by the tool, so it
        // does not count as churn.
        if rename.trim().eq_ignore_ascii_case(field.trim()) || rename.trim().is_empty() {
            continue;
        }
        if let Some(resource) = object.get("resource").and_then(Value::as_str) {
            resources.push(resource.to_ascii_lowercase());
        }
    }
    resources.sort();
    resources.dedup();
    // Return the first renamed resource; one call normally targets one resource.
    resources.into_iter().next().map(|resource| format!("sync_schema:{resource}"))
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

/// The message a run ends with when it used up its turn budget. It names the
/// budget and tells the user how to continue, because the previous wording
/// ("Reached maximum allowed turns") read like an error and gave no next step.
fn max_turns_notice(max_turns: u32) -> String {
    format!(
        "Stopped after {max_turns} turns before finishing. Send **continue** to resume from where I left off."
    )
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

/// Builds the catalogue of available skills appended to a system prompt.
///
/// Returns an empty string when there are no skills at all so callers can opt out
/// of the section altogether. `project_skills` are the module-installed skills; a
/// note explaining where they came from follows them.
fn skills_summary(skills: &[Box<dyn Skill>], project_skills: &[&dyn Skill]) -> String {
    if skills.is_empty() && project_skills.is_empty() {
        return String::new();
    }
    let mut summary = String::from("\n\n=== AVAILABLE SKILLS ===\nYou have access to the following skills that teach you how to perform specific tasks:\n\n");
    for skill in skills {
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
    for skill in project_skills {
        summary.push_str(&format!(
            "**{}** (installed module)\nDescription: {}\n",
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
        "To use a skill, call the load_skill tool with the skill name. A skill may reference supporting documents; load them with the load_skill tool by passing the skill name and the file path.\n=== END SKILLS ===\n",
    );
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    fn system_message(content: &str) -> LLMMessage {
        LLMMessage {
            role: LLMMessageRole::System,
            content: content.to_owned(),
            tool_call_id: None,
            tool_name: None,
            tool_args: None,
            tool_calls: None,
        }
    }

    #[test]
    fn skills_summary_is_empty_without_skills() {
        assert!(skills_summary(&[], &[]).is_empty());
    }

    #[test]
    fn skills_summary_lists_the_default_catalogue() {
        let skills = super::super::default_skills();
        let summary = skills_summary(&skills, &[]);
        assert!(summary.contains("=== AVAILABLE SKILLS ==="));
        assert!(summary.contains("crud_creation"));
        assert!(summary.contains("=== END SKILLS ==="));
    }

    #[test]
    fn skills_summary_does_not_advertise_reference_documents() {
        let skills = super::super::default_skills();
        let summary = skills_summary(&skills, &[]);
        assert!(!summary.contains("Documents (load on demand)"));
        assert!(!summary.contains("also pass its document name"));
    }

    #[test]
    fn empty_options_skills_do_not_touch_the_system_prompt() {
        let agent = AgentService::with_skills(Arc::new(StubLlm), Vec::new());
        let mut messages = vec![system_message("base prompt")];
        agent.attach_skills_summary(&mut messages, &[], &[]);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "base prompt");
    }

    #[test]
    fn agent_defaults_apply_when_options_do_not_override_skills() {
        let options = AgentOptions::default();
        assert!(options.skills.is_none());
    }

    /// Minimal `LLMService` so the summary tests never need a provider.
    struct StubLlm;

    #[async_trait]
    impl LLMService for StubLlm {
        async fn execute_prompt(
            &self,
            _prompt: &str,
            _context: Option<&Map<String, Value>>,
        ) -> LlmResult<String> {
            Ok(String::new())
        }

        async fn execute_prompt_with_tools(
            &self,
            _messages: &[LLMMessage],
            _tools: &[Box<dyn Tool>],
            _context: Option<&Map<String, Value>>,
        ) -> LlmResult<Value> {
            Ok(Value::Null)
        }
    }

    /// Always emits the same `failing_tool` call with identical arguments.
    struct RepeatingLlm;
    #[async_trait]
    impl LLMService for RepeatingLlm {
        async fn execute_prompt(
            &self,
            _prompt: &str,
            _context: Option<&Map<String, Value>>,
        ) -> LlmResult<String> {
            Ok(String::new())
        }

        async fn execute_prompt_with_tools(
            &self,
            _messages: &[LLMMessage],
            _tools: &[Box<dyn Tool>],
            _context: Option<&Map<String, Value>>,
        ) -> LlmResult<Value> {
            Ok(json!({
                "content": "",
                "tool_calls": [{
                    "id": "call-1",
                    "type": "function",
                    "function": {
                        "name": "failing_tool",
                        "arguments": { "projectPath": "/tmp/project", "value": 1 }
                    }
                }]
            }))
        }
    }

    struct FailingTool {
        parameters: Map<String, Value>,
    }

    impl FailingTool {
        fn new() -> Self {
            Self {
                parameters: Map::new(),
            }
        }
    }

    #[async_trait]
    impl Tool for FailingTool {
        fn name(&self) -> &str {
            "failing_tool"
        }
        fn description(&self) -> &str {
            "Always fails."
        }
        fn parameters(&self) -> &Map<String, Value> {
            &self.parameters
        }
        async fn execute(&self, _args: Value) -> LlmResult<Value> {
            Err("`rename` is a no-op; put properties in `changes`".into())
        }
    }

    fn agent_options() -> AgentOptions {
        AgentOptions {
            project_path: Some("/tmp/project".to_string()),
            log_conversation: false,
            ..Default::default()
        }
    }

    /// Emits a `sync_schema` rename whose target changes on every turn, matching
    /// the sales-system incident (`saleId` -> `saleRef` -> `saleId` -> `sale_id_tmp`).
    struct RenameLoopLlm {
        names: Vec<String>,
    }

    #[async_trait]
    impl LLMService for RenameLoopLlm {
        async fn execute_prompt(
            &self,
            _prompt: &str,
            _context: Option<&Map<String, Value>>,
        ) -> LlmResult<String> {
            Ok(String::new())
        }

        async fn execute_prompt_with_tools(
            &self,
            messages: &[LLMMessage],
            _tools: &[Box<dyn Tool>],
            _context: Option<&Map<String, Value>>,
        ) -> LlmResult<Value> {
            // One rename per turn; the rename target cycles through `self.names`.
            let turn = messages
                .iter()
                .filter(|message| matches!(message.role, LLMMessageRole::Tool))
                .count();
            let target = self
                .names
                .get(turn % self.names.len())
                .cloned()
                .unwrap_or_else(|| "saleId".to_string());
            Ok(json!({
                "content": "",
                "tool_calls": [{
                    "id": format!("call-{turn}"),
                    "type": "function",
                    "function": {
                        "name": "sync_schema",
                        "arguments": {
                            "projectPath": "/tmp/project",
                            "operations": [{
                                "action": "updateField",
                                "resource": "payments",
                                "field": "saleId",
                                "rename": target
                            }]
                        }
                    }
                }]
            }))
        }
    }

    /// Succeeds no matter what, so only the oscillation guard can stop the run.
    struct AlwaysOkTool {
        parameters: Map<String, Value>,
    }

    #[async_trait]
    impl Tool for AlwaysOkTool {
        fn name(&self) -> &str {
            "sync_schema"
        }
        fn description(&self) -> &str {
            "Pretends to edit the schema."
        }
        fn parameters(&self) -> &Map<String, Value> {
            &self.parameters
        }
        async fn execute(&self, _args: Value) -> LlmResult<Value> {
            Ok(json!({ "success": true, "message": "Schema sync completed successfully." }))
        }
    }

    /// The incident's shape: every rename succeeds, so the identical-call guard
    /// never fires and only the per-resource rename counter can stop the loop.
    #[tokio::test]
    async fn repeated_renames_on_one_resource_stop_the_run() {
        let llm = Arc::new(RenameLoopLlm {
            names: vec![
                "saleRef".to_string(),
                "saleId_back".to_string(),
                "sale_id_tmp".to_string(),
                "sale_id_tmp2".to_string(),
                "sale_id_tmp3".to_string(),
                "sale_id_tmp4".to_string(),
            ],
        });
        let agent = AgentService::with_skills(llm, Vec::new());

        let result = agent
            .run_agent(
                vec![
                    system_message("system"),
                    LLMMessage {
                        role: LLMMessageRole::User,
                        content: "change the type".to_string(),
                        tool_call_id: None,
                        tool_name: None,
                        tool_args: None,
                        tool_calls: None,
                    },
                ],
                vec![Box::new(AlwaysOkTool {
                    parameters: Map::new(),
                })],
                AgentOptions {
                    max_turns: Some(30),
                    ..agent_options()
                },
            )
            .await
            .unwrap();

        assert!(result.stalled, "expected the run to be marked stalled");
        assert!(!result.hit_max_turns, "the guard must fire before the budget");
    }

    #[test]
    fn schema_rename_target_only_counts_real_renames() {
        let rename = json!({
            "operations": [{
                "action": "updateField",
                "resource": "payments",
                "field": "saleId",
                "rename": "saleRef"
            }]
        });
        assert_eq!(
            schema_rename_target("sync_schema", &rename).as_deref(),
            Some("sync_schema:payments")
        );

        // A no-op rename is rejected by the tool, so it must not count as churn.
        let no_op = json!({
            "operations": [{
                "action": "updateField",
                "resource": "payments",
                "field": "saleId",
                "rename": "saleId"
            }]
        });
        assert_eq!(schema_rename_target("sync_schema", &no_op), None);

        // A plain property edit is not churn either.
        let edit = json!({
            "operations": [{
                "action": "updateField",
                "resource": "payments",
                "field": "saleId",
                "changes": { "required": true }
            }]
        });
        assert_eq!(schema_rename_target("sync_schema", &edit), None);
    }

    #[test]
    fn fingerprint_ignores_argument_key_order() {
        let a = json!({ "b": 2, "a": 1 });
        let b = json!({ "a": 1, "b": 2 });
        assert_eq!(
            fingerprint_tool_call("sync_schema", &a),
            fingerprint_tool_call("sync_schema", &b)
        );
    }

    /// The production failure mode: the model re-sent a byte-identical call that
    /// had just failed. The second attempt must not be executed again, and the
    /// third must stop the run instead of burning the turn budget.
    #[tokio::test]
    async fn identical_failing_tool_call_is_intercepted_and_then_stops_the_run() {
        let agent = AgentService::with_skills(Arc::new(RepeatingLlm), Vec::new());

        let result = agent
            .run_agent(
                vec![
                    system_message("system"),
                    LLMMessage {
                        role: LLMMessageRole::User,
                        content: "do the thing".to_string(),
                        tool_call_id: None,
                        tool_name: None,
                        tool_args: None,
                        tool_calls: None,
                    },
                ],
                vec![Box::new(FailingTool::new())],
                AgentOptions {
                    max_turns: Some(30),
                    ..agent_options()
                },
            )
            .await
            .expect("agent run");

        assert!(result.stalled, "expected the run to be marked stalled");
        assert!(!result.hit_max_turns);
        assert!(result.content.contains("same tool call kept failing"));
        // Executed once, intercepted once, stopped once.
        assert_eq!(result.tool_calls.len(), 3);
    }

    /// Always asks for a tool that succeeds, so the run can only end by using up
    /// its turn budget. Mirrors the production run in log 1795440427291-39040,
    /// where exploration consumed every turn without a single write.
    struct EndlessLlm {
        turn: std::sync::atomic::AtomicU32,
    }

    #[async_trait]
    impl LLMService for EndlessLlm {
        async fn execute_prompt(
            &self,
            _prompt: &str,
            _context: Option<&Map<String, Value>>,
        ) -> LlmResult<String> {
            Ok(String::new())
        }

        async fn execute_prompt_with_tools(
            &self,
            _messages: &[LLMMessage],
            _tools: &[Box<dyn Tool>],
            _context: Option<&Map<String, Value>>,
        ) -> LlmResult<Value> {
            let turn = self.turn.fetch_add(1, Ordering::Relaxed);
            Ok(json!({
                // A truncated preamble: the last thought never became an answer.
                "content": "Let me read one more file before writing.\n",
                "tool_calls": [{
                    "id": format!("call-{turn}"),
                    "type": "function",
                    "function": {
                        "name": "ok_tool",
                        "arguments": { "projectPath": "/tmp/project", "index": turn }
                    }
                }]
            }))
        }
    }

    struct OkTool {
        parameters: Map<String, Value>,
    }

    #[async_trait]
    impl Tool for OkTool {
        fn name(&self) -> &str {
            "ok_tool"
        }
        fn description(&self) -> &str {
            "Always succeeds."
        }
        fn parameters(&self) -> &Map<String, Value> {
            &self.parameters
        }
        async fn execute(&self, _args: Value) -> LlmResult<Value> {
            Ok(json!({ "ok": true }))
        }
    }

    #[tokio::test]
    async fn hitting_the_turn_budget_reports_the_limit_and_replaces_the_preamble() {
        let agent = AgentService::with_skills(
            Arc::new(EndlessLlm {
                turn: std::sync::atomic::AtomicU32::new(0),
            }),
            Vec::new(),
        );

        let result = agent
            .run_agent(
                vec![
                    system_message("system"),
                    LLMMessage {
                        role: LLMMessageRole::User,
                        content: "do the thing".to_string(),
                        tool_call_id: None,
                        tool_name: None,
                        tool_args: None,
                        tool_calls: None,
                    },
                ],
                vec![Box::new(OkTool {
                    parameters: Map::new(),
                })],
                AgentOptions {
                    max_turns: Some(3),
                    ..agent_options()
                },
            )
            .await
            .expect("agent run");

        assert!(result.hit_max_turns, "expected the run to hit the budget");
        assert_eq!(result.max_turns, 3);
        // The trailing preamble must not survive as the answer.
        assert!(!result.content.contains("one more file"));
        assert!(result.content.contains("Stopped after 3 turns"));
        assert!(result.content.contains("continue"));
        // Only executed turns are reported; the 3rd turn's calls never ran.
        assert_eq!(result.tool_calls.len(), 2);
    }
}
