import { ChangeEvent, FormEvent, KeyboardEvent, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
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

function ThinkingBlock({ content, isRunning }: { content: string; isRunning: boolean }) {
  const [isOpen, setIsOpen] = useState(false);

  return (
    <article className={`thinking-block ${isRunning ? "running" : "complete"}`}>
      <button
        className="thinking-summary"
        type="button"
        onClick={() => setIsOpen((open) => !open)}
        aria-expanded={isOpen}
      >
        <span className="thinking-status">
          {isRunning ? <span className="thinking-spinner" aria-hidden="true" /> : <span className="thinking-brain" aria-hidden="true">🧠</span>}
        </span>
        <span className="thinking-caption">{isRunning ? "Thinking…" : "Thinking"}</span>
        <span className="thinking-chevron" aria-hidden="true">{isOpen ? "⌃" : "⌄"}</span>
      </button>
      {isOpen && (
        <div className="thinking-details">
          <MessageContent content={content} />
        </div>
      )}
    </article>
  );
}

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
type ProviderSummary = {
  id: string;
  key: string;
  provider_type: string;
  name: string;
  title: string;
  detail: string;
  api_key_set: boolean;
  models: string[];
  reasoning?: string | null;
};

export const REASONING_MODES = [
  { value: "off", label: "Off" },
  { value: "minimal", label: "Minimal" },
  { value: "low", label: "Low" },
  { value: "medium", label: "Medium" },
  { value: "high", label: "High" },
  { value: "xhigh", label: "XHigh" },
] as const;

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
  onModelChange: (provider: string, model: string, reasoning?: string) => void;
  mode: "implement" | "plan";
  onModeChange: (mode: "implement" | "plan") => void;
  onProceedToImplement: () => void;
  onStop?: () => void;
};

export default function Chat({ projectTitle, sessionTitle, sessions, activeSessionId, messages, draft, isSending, isStarting, error, onBack, onStart, onNewSession, onSelectSession, onDeleteSession, onDraftChange, onSend, attachments, onAttach, onRemoveAttachment, activeProvider, activeModel, onModelChange, mode, onModeChange, onProceedToImplement, onStop }: ChatProps) {
  const fileInput = useRef<HTMLInputElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const [providers, setProviders] = useState<ProviderSummary[]>([]);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);

  // Automatically adjust textarea height based on content
  useEffect(() => {
    const textarea = textareaRef.current;
    if (!textarea) return;
    textarea.style.height = "auto";
    const maxHeight = Math.round(window.innerHeight * 0.45);
    const nextHeight = Math.min(textarea.scrollHeight, maxHeight);
    textarea.style.height = `${Math.max(nextHeight, 48)}px`;
    textarea.style.overflowY = textarea.scrollHeight > maxHeight ? "auto" : "hidden";
  }, [draft]);

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
      await Promise.all(configured.map((p) => {
        const identifier = p.id || p.key;
        return invoke("fetch_models", { provider: identifier })
          .catch((reason: unknown) => console.warn("[Chat] fetch_models failed", { provider: identifier, reason }));
      }));
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
    if (first?.models.length) onModelChange(first.id || first.key, first.models[0]);
  }, [activeModel, providers]);

  function handleKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      if (!isSending && draft.trim()) {
        const form = event.currentTarget.form;
        if (form) form.requestSubmit();
      }
    }
  }

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
    if (message.role === "thought") {
      const isLastMessage = index === messages.length - 1;
      renderedMessages.push(
        <ThinkingBlock
          content={message.content}
          isRunning={isSending && isLastMessage}
          key={`thought-${index}`}
        />
      );
      continue;
    }
    const isPlan = (message.data as { isPlan?: boolean } | undefined)?.isPlan;
    const planPath = (message.data as { planPath?: string } | undefined)?.planPath;
    if (isPlan) {
      renderedMessages.push(
        <article className="message assistant plan-message" key={`plan-${index}`}>
          <div className="plan-header">
            <div className="plan-title-badge">
              <span className="plan-icon" aria-hidden="true">📋</span>
              <span className="plan-title">Implementation Plan</span>
            </div>
            {planPath && <span className="plan-path" title={planPath}>Saved to: {planPath.split(/[\\/]/).pop()}</span>}
          </div>
          <MessageContent content={message.content} />
          <div className="plan-actions">
            <button
              className="plan-proceed-btn"
              type="button"
              disabled={isSending}
              onClick={onProceedToImplement}
            >
              <span aria-hidden="true">⚡</span> Proceed to Implement
            </button>
          </div>
        </article>
      );
      continue;
    }
    renderedMessages.push(<article className={`message ${message.role}`} key={`${message.role}-${index}`}><span>{message.role === "user" ? "You" : "Eggshell"}</span><MessageContent content={message.content} /></article>);
  }
  return <main className="chat-layout">
    <aside className="chat-sidebar"><button className="back-button" type="button" onClick={onBack}>← Projects</button><div className="project-name"><p className="eyebrow">Project</p><h2>{projectTitle}</h2></div><button className="start-button" type="button" onClick={onStart} disabled={isStarting}>{isStarting ? "Starting…" : "Start"}</button><aside className="login-details" aria-label="Admin login details"><p className="eyebrow">Admin login</p><dl><div><dt>Username</dt><dd>admin@dummy-project.com</dd></div><div><dt>Password</dt><dd>12345678</dd></div></dl></aside><button className="new-chat-button" type="button" onClick={onNewSession}>+ New session</button><nav className="session-list" aria-label="Chat sessions">{sessions.map((session) => <div className={activeSessionId === session.id ? "session-row active" : "session-row"} key={session.id}><button className={activeSessionId === session.id ? "session-item active" : "session-item"} type="button" onClick={() => onSelectSession(session.id)}>{session.title}</button><button className="session-delete-button" type="button" aria-label={`Delete ${session.title}`} onClick={() => onDeleteSession(session.id)}>×</button></div>)}{!sessions.length && <p className="sessions-empty">Your first message will create a session.</p>}</nav></aside>
    <section className="chat-panel">
      <header className="chat-header">
        <div className="chat-heading">
          <h1>{sessionTitle ?? "New session"}</h1>
          <p>{sessionTitle ? "Dummy assistant" : "Start a conversation"}</p>
        </div>
        <div className="chat-header-actions">
          <button className="icon-button settings-button" type="button" aria-label="Settings" onClick={() => setIsSettingsOpen(true)}>⚙</button>
        </div>
      </header>
      <div className="message-list" aria-live="polite">
        {!messages.length && <div className="chat-empty"><h2>How can I help?</h2><p>Send a message to begin.</p></div>}
        {renderedMessages}
      </div>
      {error && <p className="chat-error" role="alert">{error}</p>}
      <form className="composer" onSubmit={onSend}>
        <input
          ref={fileInput}
          type="file"
          multiple
          hidden
          onChange={(event) => {
            onAttach(Array.from(event.target.files ?? []).map(({ name }) => name));
            event.target.value = "";
          }}
        />
        <div className="composer-container">
          {attachments.length > 0 && (
            <ul className="attachment-list" aria-label="Attached files">
              {attachments.map((name) => (
                <li className="attachment-chip" key={name}>
                  <span className="attachment-name" title={name}>{name}</span>
                  <button type="button" aria-label={`Remove ${name}`} onClick={() => onRemoveAttachment(name)}>×</button>
                </li>
              ))}
            </ul>
          )}
          <textarea
            ref={textareaRef}
            className="composer-textarea"
            value={draft}
            onChange={(e: ChangeEvent<HTMLTextAreaElement>) => onDraftChange(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Type a message… (Press Enter to send, Shift+Enter for new line)"
            rows={1}
            aria-label="Message"
          />
          <div className="composer-bottom-bar">
            <div className="composer-tools">
              <button
                className="composer-attach-btn"
                type="button"
                aria-label="Attach files"
                title="Attach files"
                onClick={() => fileInput.current?.click()}
                disabled={isSending}
              >
                <span className="composer-attach-icon">📎</span>
                <span className="composer-attach-label">Attach</span>
              </button>
              <div className={`composer-mode-picker ${mode === "plan" ? "plan-active" : ""}`}>
                <span className="composer-mode-icon">{mode === "plan" ? "📋" : "⚙️"}</span>
                <span className="composer-mode-label">Mode:</span>
                <select
                  className="composer-mode-select"
                  value={mode}
                  aria-label="Mode Selection"
                  disabled={isSending}
                  onChange={({ target }) => onModeChange(target.value as "implement" | "plan")}
                >
                  <option value="implement">Implement</option>
                  <option value="plan">Plan</option>
                </select>
              </div>
              {selectableProviders.length > 0 && (
                <>
                  <div className="composer-model-picker">
                    <span className="composer-model-icon">✨</span>
                    <select
                      className="composer-model-select"
                      value={`${activeProvider}:${activeModel}`}
                      aria-label="Model Selection"
                      onChange={({ target }) => {
                        const [provider, ...rest] = target.value.split(":");
                        const currentReasoning = providers.find((p) => p.key === provider)?.reasoning ?? "off";
                        onModelChange(provider, rest.join(":"), currentReasoning || "off");
                      }}
                    >
                      {selectableProviders.map((p) => {
                        const id = p.id || p.key;
                        const label = p.title || p.name;
                        return (
                          <optgroup key={id} label={label}>
                            {p.models.map((model) => (
                              <option key={`${id}:${model}`} value={`${id}:${model}`}>
                                {model}
                              </option>
                            ))}
                          </optgroup>
                        );
                      })}
                    </select>
                  </div>
                  <div className="composer-reasoning-picker">
                    <span className="composer-reasoning-icon">🧠</span>
                    <span className="composer-reasoning-label">Reasoning:</span>
                    <select
                      className="composer-reasoning-select"
                      value={providers.find((p) => (p.id || p.key) === activeProvider)?.reasoning || "off"}
                      aria-label="Reasoning Effort"
                      onChange={({ target }) => {
                        const nextReasoning = target.value;
                        setProviders((current) =>
                          current.map((p) => (p.key === activeProvider ? { ...p, reasoning: nextReasoning } : p))
                        );
                        onModelChange(activeProvider, activeModel, nextReasoning);
                      }}
                    >
                      {REASONING_MODES.map(({ value, label }) => (
                        <option key={value} value={value}>
                          {label}
                        </option>
                      ))}
                    </select>
                  </div>
                </>
              )}
            </div>
            {isSending ? (
              <button
                className="composer-stop-btn"
                type="button"
                onClick={onStop}
                aria-label="Stop generation"
              >
                <span className="composer-stop-icon" aria-hidden="true">■</span> Stop
              </button>
            ) : (
              <button className="composer-send-btn" disabled={!draft.trim()} type="submit">
                Send
              </button>
            )}
          </div>
        </div>
      </form>
    </section>
    <ReportMenu screenName="Project" />
    <SettingsPopup isOpen={isSettingsOpen} onClose={() => { setIsSettingsOpen(false); loadProviders(); }} />
  </main>;
}
