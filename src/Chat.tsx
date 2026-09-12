import { FormEvent, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { CodeMirrorMarkdownEditor } from "@latentic/live-markdown";
import ReportMenu from "./ReportMenu";
import SettingsPopup from "./SettingsPopup";

export type ChatMessage = {
  role: "user" | "assistant" | "thought" | "tool_call" | "tool_result";
  content: string;
  data?: unknown;
};

type ToolEvent = {
  name?: string;
  arguments?: unknown;
  result?: unknown;
};

type ToolMessageFormatter = (event: ToolEvent) => string;

// Add a formatter here for each tool that needs a human-friendly description.
// Tools without a formatter keep the generic, JSON-free fallback below.
const toolCallFormatters: Record<string, ToolMessageFormatter> = {
  load_skill: ({ arguments: args }) => {
    const skillName = (args as { skillName?: unknown } | undefined)?.skillName;
    return `Loaded skill: ${typeof skillName === "string" ? skillName : "unknown"}`;
  },
};

function formatToolMessage(event: ToolEvent) {
  const name = event.name ?? "tool";
  return toolCallFormatters[name]?.(event) ?? `Using tool: ${name}`;
}

export function eventContent(type: string, data: unknown) {
  const event = data as ToolEvent & { content?: string };
  if (type === "thought") return event.content ?? "";
  if (type === "tool_call") return formatToolMessage(event);
  if (type === "tool_result") return `Completed tool: ${event.name ?? "tool"}`;
  return JSON.stringify(data);
}

function jsonForDisplay(value: unknown) {
  if (value === undefined) return "No data returned.";
  try { return JSON.stringify(value, null, 2); }
  catch { return String(value); }
}

function toolFailed(result: ToolEvent | undefined) {
  if (!result || !result.result || typeof result.result !== "object") return false;
  return "error" in result.result;
}

function MessageContent({ content }: { content: string }) {
  return <div className="message-content">
    <Markdown remarkPlugins={[remarkGfm]}>{content}</Markdown>
  </div>;
}

type ToolRunProps = { call: ChatMessage; result?: ChatMessage };

function ToolRun({ call, result }: ToolRunProps) {
  const [isOpen, setIsOpen] = useState(false);
  const callEvent = call.data as ToolEvent;
  const resultEvent = result?.data as ToolEvent | undefined;
  const isRunning = !result;
  const failed = toolFailed(resultEvent);
  const status = isRunning ? "Running" : failed ? "Failed" : "Completed";

  return <article className={`tool-run ${isRunning ? "running" : failed ? "failed" : "complete"}`}>
    <button className="tool-run-summary" type="button" onClick={() => setIsOpen((open) => !open)} aria-expanded={isOpen}>
      <span className="tool-run-status" aria-label={status}>
        {isRunning ? <span className="tool-spinner" aria-hidden="true" /> : failed ? <span className="tool-x" aria-hidden="true">×</span> : <span className="tool-check" aria-hidden="true">✓</span>}
      </span>
      <span className="tool-run-name">{formatToolMessage(callEvent)}</span>
      <span className="tool-run-state">{status}</span>
      <span className="tool-run-chevron" aria-hidden="true">{isOpen ? "⌃" : "⌄"}</span>
    </button>
    {isOpen && <div className="tool-run-details">
      <section><h3>Arguments</h3><pre>{jsonForDisplay(callEvent.arguments)}</pre></section>
      {!isRunning && <section><h3>{failed ? "Error" : "Result"}</h3><pre>{jsonForDisplay(resultEvent?.result)}</pre></section>}
    </div>}
  </article>;
}

// Mirrors `providers::ProviderSummary` as returned by `load_setup_state`.
type ProviderSummary = { key: string; name: string; detail: string; api_key_set: boolean; models: string[] };

type ChatProps = {
  projectTitle: string;
  sessionTitle?: string;
  sessions: { id: number; title: string }[];
  activeSessionId?: number;
  messages: ChatMessage[];
  draft: string;
  isSending: boolean;
  isStarting: boolean;
  error: string;
  onBack: () => void;
  onStart: () => void;
  onNewSession: () => void;
  onSelectSession: (id: number) => void;
  onDeleteSession: (id: number) => void;
  onDraftChange: (draft: string) => void;
  onSend: (event: FormEvent<HTMLFormElement>) => void;
  attachments: string[];
  onAttach: (files: string[]) => void;
  onRemoveAttachment: (name: string) => void;
  /** The provider owning the model currently answering. */
  activeProvider: string;
  /** The first model in the active provider's list is the backend's current default. */
  activeModel: string;
  onModelChange: (provider: string, model: string) => void;
};

export default function Chat({ projectTitle, sessionTitle, sessions, activeSessionId, messages, draft, isSending, isStarting, error, onBack, onStart, onNewSession, onSelectSession, onDeleteSession, onDraftChange, onSend, attachments, onAttach, onRemoveAttachment, activeProvider, activeModel, onModelChange }: ChatProps) {
  const fileInput = useRef<HTMLInputElement>(null);
  const [providers, setProviders] = useState<ProviderSummary[]>([]);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);

  // The model list lives in config.yaml; the chat header needs it to offer the
  // switcher. Reading it here keeps App.tsx out of provider business.
  async function loadProviders() {
    const { providers: loaded } = await invoke<{ providers: ProviderSummary[] }>("load_setup_state");
    setProviders(loaded);
    return loaded;
  }

  // Opening the chat refreshes the model list for every configured provider.
  // The backend caches each result for 24 hours and answers from that cache, so
  // this is only an upstream request when the cache has gone stale. Fetching is
  // best-effort: a provider that cannot be reached leaves the configured models
  // in place rather than blocking the screen.
  async function refreshModels() {
    try {
      const current = await loadProviders();
      const configured = current.filter(({ api_key_set }) => api_key_set);
      if (configured.length === 0) return;
      await Promise.all(configured.map(({ key }) => invoke("fetch_models", { provider: key })
        .catch((reason: unknown) => console.warn("[Chat] fetch_models failed", { provider: key, reason }))));
      await loadProviders();
    } catch (reason) {
      console.error("[Chat] model refresh failed", { reason });
    }
  }

  useEffect(() => { void refreshModels(); }, []);

  const selectableProviders = providers.filter(({ models }) => models.length > 0);

  // Models only reach the picker once a fetch has populated them, which happens
  // after App.tsx reads its initial defaults. Adopt the first available model so
  // the picker and the backend agree on what answers.
  useEffect(() => {
    if (activeModel) return;
    const first = selectableProviders[0];
    if (first?.models.length) onModelChange(first.key, first.models[0]);
  }, [activeModel, providers]);
  const renderedMessages = [];
  for (let index = 0; index < messages.length; index += 1) {
    const message = messages[index];
    if (message.role === "tool_call") {
      const nextMessage = messages[index + 1];
      const result = nextMessage?.role === "tool_result" ? nextMessage : undefined;
      renderedMessages.push(<ToolRun call={message} result={result} key={`tool-${index}`} />);
      if (result) index += 1;
      continue;
    }
    // Tool results are normally consumed by the preceding call. Retain orphaned
    // results so older or interrupted conversations do not silently lose data.
    if (message.role === "tool_result") {
      renderedMessages.push(<article className="message tool_result" key={`tool-result-${index}`}><span>Eggshell</span><MessageContent content={message.content} /></article>);
      continue;
    }
    renderedMessages.push(<article className={`message ${message.role}`} key={`${message.role}-${index}`}><span>{message.role === "user" ? "You" : "Eggshell"}</span><MessageContent content={message.content} /></article>);
  }
  return <main className="chat-layout">
    <aside className="chat-sidebar"><button className="back-button" type="button" onClick={onBack}>← Projects</button><div className="project-name"><p className="eyebrow">Project</p><h2>{projectTitle}</h2></div><button className="start-button" type="button" onClick={onStart} disabled={isStarting}>{isStarting ? "Starting…" : "Start"}</button><aside className="login-details" aria-label="Admin login details"><p className="eyebrow">Admin login</p><dl><div><dt>Username</dt><dd>admin@dummy-project.com</dd></div><div><dt>Password</dt><dd>12345678</dd></div></dl></aside><button className="new-chat-button" type="button" onClick={onNewSession}>+ New session</button><nav className="session-list" aria-label="Chat sessions">{sessions.map((session) => <div className={activeSessionId === session.id ? "session-row active" : "session-row"} key={session.id}><button className={activeSessionId === session.id ? "session-item active" : "session-item"} type="button" onClick={() => onSelectSession(session.id)}>{session.title}</button><button className="session-delete-button" type="button" aria-label={`Delete ${session.title}`} onClick={() => onDeleteSession(session.id)}>×</button></div>)}{!sessions.length && <p className="sessions-empty">Your first message will create a session.</p>}</nav></aside>
    <section className="chat-panel"><header className="chat-header"><div className="chat-heading"><h1>{sessionTitle ?? "New session"}</h1><p>{sessionTitle ? "Dummy assistant" : "Start a conversation"}</p></div><div className="chat-header-actions">{selectableProviders.length > 0 && <label className="model-picker">Model<select value={`${activeProvider}:${activeModel}`} onChange={({ target }) => { const [provider, ...rest] = target.value.split(":"); onModelChange(provider, rest.join(":")); }}>{selectableProviders.map(({ key, name, models }) => <optgroup key={key} label={name}>{models.map((model) => <option key={`${key}:${model}`} value={`${key}:${model}`}>{model}</option>)}</optgroup>)}</select></label>}<button className="icon-button settings-button" type="button" aria-label="Settings" onClick={() => setIsSettingsOpen(true)}>&#9881;</button></div></header><div className="message-list" aria-live="polite">{!messages.length && <div className="chat-empty"><h2>How can I help?</h2><p>Send a message to begin.</p></div>}{renderedMessages}</div>{error && <p className="chat-error" role="alert">{error}</p>}<form className="composer" onSubmit={onSend}><input ref={fileInput} type="file" multiple hidden onChange={(event) => { onAttach(Array.from(event.target.files ?? []).map(({ name }) => name)); event.target.value = ""; }} /><button className="attach-button" type="button" aria-label="Attach files" onClick={() => fileInput.current?.click()} disabled={isSending}>📎</button><div className="composer-main">{attachments.length > 0 && <ul className="attachment-list" aria-label="Attached files">{attachments.map((name) => <li className="attachment-chip" key={name}><span className="attachment-name" title={name}>{name}</span><button type="button" aria-label={`Remove ${name}`} onClick={() => onRemoveAttachment(name)}>×</button></li>)}</ul>}<CodeMirrorMarkdownEditor value={draft} onChange={onDraftChange} mode="wysiwyg" aria-label="Message" /></div><button className="add-button" disabled={isSending || !draft.trim()} type="submit">{isSending ? "Sending…" : "Send"}</button></form></section>
    <ReportMenu screenName="Project" />
    <SettingsPopup isOpen={isSettingsOpen} onClose={() => { setIsSettingsOpen(false); loadProviders(); }} />
  </main>;
}
