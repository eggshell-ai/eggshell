import { ChangeEvent, FormEvent, KeyboardEvent, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import ReportMenu from "./ReportMenu";
import SettingsPopup from "./SettingsPopup";
import FeedbackPopup from "./FeedbackPopup";
import IterationLoopCard, { IterationLoopData } from "./IterationLoopCard";
import MaxTasksCard, { MaxPlanData, MaxTask } from "./MaxTasksCard";

export type ChatMessage = {
  role: "user" | "assistant" | "thought" | "tool_call" | "tool_result";
  content: string;
  data?: unknown;
};

/** A sign-in account for the active project (mirrored from the Symfony backend). */
export type ChatUser = {
  id: number;
  project_id: number;
  email: string;
  role: string;
  password: string;
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
    const loadArgs = args as { skillName?: unknown; document?: unknown } | undefined;
    const skillName = typeof loadArgs?.skillName === "string" ? loadArgs.skillName : "unknown";
    const document = typeof loadArgs?.document === "string" ? loadArgs.document : undefined;
    return document ? `Loaded skill: ${skillName} (${document})` : `Loaded skill: ${skillName}`;
  },
  browser_open: ({ arguments: args }) => {
    const url = (args as { url?: string } | undefined)?.url || "http://localhost:4000";
    return `Opening browser at ${url}`;
  },
  browser_navigate: ({ arguments: args }) => {
    const url = (args as { url?: string } | undefined)?.url || "";
    return `Navigating to ${url}`;
  },
  browser_click: ({ arguments: args }) => {
    const selector = (args as { selector?: string } | undefined)?.selector || "";
    return `Clicking element: ${selector}`;
  },
  browser_fill: ({ arguments: args }) => {
    const selector = (args as { selector?: string } | undefined)?.selector || "";
    return `Filling input: ${selector}`;
  },
  browser_get_page_content: () => "Inspecting page content & tables",
  browser_get_console_errors: () => "Checking browser errors",
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

type ToolRunProps = { call: ChatMessage; result?: ChatMessage };function ThinkingBlock({ content, isRunning }: { content: string; isRunning: boolean }) {
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

function UsersList({ users }: { users: ChatUser[] }) {
  const [copiedEmail, setCopiedEmail] = useState<string | null>(null);

  async function copyCredentials(user: ChatUser) {
    try {
      await navigator.clipboard.writeText(`${user.email} / ${user.password}`);
      setCopiedEmail(user.email);
      setTimeout(() => setCopiedEmail((current) => (current === user.email ? null : current)), 2000);
    } catch (reason) {
      console.error("[Chat] clipboard write failed", { reason });
    }
  }

  return (
    <aside className="login-details" aria-label="Project login details">
      <p className="eyebrow">Sign-in accounts</p>
      {users.length === 0 ? (
        <p className="users-empty">No users yet. The admin account appears once the project is created.</p>
      ) : (
        <ul className="user-list">
          {users.map((user) => (
            <li className={`user-item ${user.role === "ROLE_ADMIN" ? "is-admin" : ""}`} key={`${user.project_id}-${user.email}`}>
              <div className="user-item-header">
                <span className="user-email" title={user.email}>{user.email}</span>
                <button
                  className="user-copy-button"
                  type="button"
                  aria-label={`Copy credentials for ${user.email}`}
                  title="Copy email and password"
                  onClick={() => void copyCredentials(user)}
                >
                  {copiedEmail === user.email ? "Copied" : "Copy"}
                </button>
              </div>
              <div className="user-item-meta">
                <span className="user-role">{user.role === "ROLE_ADMIN" ? "Admin" : user.role.replace(/^ROLE_/, "")}</span>
                <span className="user-password" title="Password">{user.password || "—"}</span>
              </div>
            </li>
          ))}
        </ul>
      )}
    </aside>
  );
}

// Mirrors `providers::ProviderSummary` as returned by `load_setup_state`.
type ProviderSummary = {  id: string;
  key: string;
  provider_type: string;
  name: string;
  title: string;
  detail: string;
  api_key_set: boolean;
  models: string[];
  reasoning?: string | null;
  context_limits?: Record<string, number>;
  default_context_limit?: number;
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
  sessionTokenUsage?: number;
  messages: ChatMessage[];
  draft: string;
  isSending: boolean;
  isStarting: boolean;
  error: string;
  users: ChatUser[];
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
  mode: "implement" | "plan" | "qa" | "iterate" | "max";
  onModeChange: (mode: "implement" | "plan" | "qa" | "iterate" | "max") => void;
  onProceedToImplement: () => void;
  onStop?: () => void;
  onCompact?: () => void;
  isWrappingUp?: boolean;
  iterateStatus?: { loop?: number; stage?: string; phase?: string } | null;
  /** Current Max mode task list for the session, if any. */
  maxTasks?: MaxTask[];
  onRunMaxTasks?: () => void;
  onRetryMaxTasks?: () => void;
};

export default function Chat({ projectTitle, sessionTitle, sessions, activeSessionId, sessionTokenUsage = 0, messages, users, draft, isSending, isStarting, error, onBack, onStart, onNewSession, onSelectSession, onDeleteSession, onDraftChange, onSend, attachments, onAttach, onRemoveAttachment, activeProvider, activeModel, onModelChange, mode, onModeChange, onProceedToImplement, onStop, onCompact, isWrappingUp = false, iterateStatus = null, maxTasks = [], onRunMaxTasks, onRetryMaxTasks }: ChatProps) {
  const fileInput = useRef<HTMLInputElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const [providers, setProviders] = useState<ProviderSummary[]>([]);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [feedbackState, setFeedbackState] = useState<
    Record<number, { rating?: "thumbs_up" | "thumbs_down"; thankYou?: boolean }>
  >({});
  const [feedbackPopupTarget, setFeedbackPopupTarget] = useState<number | null>(null);

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

  // Keep track of whether the user is scrolled to the bottom so streaming auto-scrolls
  // only when the user is already at the bottom.
  const isAtBottomRef = useRef(true);
  const bottomThresholdPx = 60;

  useEffect(() => {
    function checkIfAtBottom() {
      const scrollY = window.scrollY || document.documentElement.scrollTop;
      const viewportHeight = window.innerHeight;
      const fullHeight = document.documentElement.scrollHeight;
      isAtBottomRef.current = fullHeight - (scrollY + viewportHeight) <= bottomThresholdPx;
    }

    checkIfAtBottom();
    window.addEventListener("scroll", checkIfAtBottom, { passive: true });
    return () => {
      window.removeEventListener("scroll", checkIfAtBottom);
    };
  }, []);

  // When messages update (e.g. streaming new text or tokens), scroll to bottom if user is already at bottom
  useEffect(() => {
    if (isAtBottomRef.current) {
      window.scrollTo({
        top: document.documentElement.scrollHeight,
        behavior: "instant",
      });
    }
  }, [messages]);

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

  // Determine active model context limit
  const activeProviderObj = selectableProviders.find((p) => (p.id || p.key) === activeProvider);
  const activeContextLimit = activeProviderObj?.context_limits?.[activeModel]
    ?? activeProviderObj?.default_context_limit
    ?? (activeProviderObj?.provider_type === "ollama" ? 32768 : 128000);

  const contextPercent = Math.min(100, Math.round((sessionTokenUsage / activeContextLimit) * 100));
  const isContextWarning = contextPercent >= 70 && contextPercent < 85;
  const isContextCritical = contextPercent >= 85;

  // Auto-compaction at 85% of context limit
  const hasAutoCompactedRef = useRef<number | null>(null);
  useEffect(() => {
    if (
      isContextCritical &&
      onCompact &&
      !isSending &&
      activeSessionId &&
      hasAutoCompactedRef.current !== activeSessionId
    ) {
      hasAutoCompactedRef.current = activeSessionId;
      onCompact();
    }
  }, [isContextCritical, onCompact, isSending, activeSessionId]);

  // Models only reach the picker once a fetch has populated them, which happens
  // after App.tsx reads its initial defaults. Ensure the active provider and model
  // always point to a valid, selectable provider.
  useEffect(() => {
    if (selectableProviders.length === 0) return;
    const currentProvider = selectableProviders.find((p) => (p.id || p.key) === activeProvider);
    const hasValidModel = Boolean(currentProvider && activeModel && currentProvider.models.includes(activeModel));
    if (!currentProvider || !hasValidModel) {
      const first = selectableProviders[0];
      if (first?.models.length) {
        const reasoning = first.reasoning || "off";
        onModelChange(first.id || first.key, first.models[0], reasoning);
      }
    }
  }, [activeProvider, activeModel, selectableProviders]);

  async function handleThumbsUp(index: number) {
    try {
      await invoke("submit_feedback", {
        payload: {
          rating: "thumbs_up",
          reason: null,
          notes: null,
          attachPromptHistory: false,
          promptHistory: null,
          provider: activeProvider || null,
          model: activeModel || null,
        },
      });
    } catch (err) {
      console.error("[Chat] Failed to submit thumbs_up feedback", err);
    }
    setFeedbackState((prev) => ({
      ...prev,
      [index]: { rating: "thumbs_up", thankYou: true },
    }));
    setTimeout(() => {
      setFeedbackState((prev) => ({
        ...prev,
        [index]: { ...prev[index], thankYou: false },
      }));
    }, 4000);
  }

  function handleThumbsDown(index: number) {
    setFeedbackPopupTarget(index);
  }

  function handleFeedbackSubmitted(index: number) {
    setFeedbackState((prev) => ({
      ...prev,
      [index]: { rating: "thumbs_down", thankYou: true },
    }));
    setTimeout(() => {
      setFeedbackState((prev) => ({
        ...prev,
        [index]: { ...prev[index], thankYou: false },
      }));
    }, 4000);
  }

  function renderFeedbackRow(index: number) {
    const isLastMessage = index === messages.length - 1;
    if (isSending && isLastMessage) return null;

    const status = feedbackState[index];

    return (
      <div className="message-feedback-row">
        {status?.thankYou ? (
          <span className="feedback-thank-you">Thank you for your feedback</span>
        ) : (
          <div className="feedback-actions">
            <button
              type="button"
              className={`feedback-btn thumbs-up ${status?.rating === "thumbs_up" ? "active" : ""}`}
              aria-label="Thumbs up"
              title="Thumbs up"
              onClick={() => void handleThumbsUp(index)}
            >
              <svg
                width="15"
                height="15"
                viewBox="0 0 24 24"
                fill={status?.rating === "thumbs_up" ? "currentColor" : "none"}
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
                aria-hidden="true"
              >
                <path d="M7 10v12" />
                <path d="M15 5.88 14 10h5.83a2 2 0 0 1 1.92 2.56l-2.33 8A2 2 0 0 1 17.5 22H4a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2h3" />
                <path d="M7 10V4.5a2.5 2.5 0 0 1 5 0V10" />
              </svg>
            </button>
            <button
              type="button"
              className={`feedback-btn thumbs-down ${status?.rating === "thumbs_down" ? "active" : ""}`}
              aria-label="Thumbs down"
              title="Thumbs down"
              onClick={() => handleThumbsDown(index)}
            >
              <svg
                width="15"
                height="15"
                viewBox="0 0 24 24"
                fill={status?.rating === "thumbs_down" ? "currentColor" : "none"}
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
                aria-hidden="true"
              >
                <path d="M17 14V2" />
                <path d="M9 18.12 10 14H4.17a2 2 0 0 1-1.92-2.56l2.33-8A2 2 0 0 1 6.5 2H20a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-3" />
                <path d="M17 14v5.5a2.5 2.5 0 0 1-5 0V14" />
              </svg>
            </button>
          </div>
        )}
      </div>
    );
  }

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
    const isIterationLoop = (message.data as IterationLoopData | undefined)?.isIterationLoop;
    if (isIterationLoop) {
      const loopData = message.data as IterationLoopData;
      renderedMessages.push(
        <IterationLoopCard
          key={`iteration-loop-${index}`}
          data={loopData}
          isCurrent={false}
        />
      );
      continue;
    }
    const isMaxPlan = (message.data as MaxPlanData | undefined)?.isMaxPlan;
    if (isMaxPlan) {
      renderedMessages.push(
        <MaxTasksCard
          key={`max-plan-${index}`}
          data={message.data as MaxPlanData}
          tasks={maxTasks}
          isSending={isSending}
          onRun={onRunMaxTasks ?? (() => undefined)}
          onRetryFailed={onRetryMaxTasks ?? (() => undefined)}
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
          {renderFeedbackRow(index)}
        </article>
      );
      continue;
    }
    if (message.role === "assistant") {
      renderedMessages.push(
        <article className="message assistant" key={`assistant-${index}`}>
          <span>Eggshell</span>
          <MessageContent content={message.content} />
          {renderFeedbackRow(index)}
        </article>
      );
      continue;
    }
    renderedMessages.push(<article className={`message ${message.role}`} key={`${message.role}-${index}`}><span>{message.role === "user" ? "You" : "Eggshell"}</span><MessageContent content={message.content} /></article>);
  }
  return <main className="chat-layout">
    <aside className="chat-sidebar"><button className="back-button" type="button" onClick={onBack}>← Projects</button><div className="project-name"><p className="eyebrow">Project</p><h2>{projectTitle}</h2></div><button className="start-button" type="button" onClick={onStart} disabled={isStarting}>{isStarting ? "Starting…" : "Start"}</button><UsersList users={users} /><button className="new-chat-button" type="button" onClick={onNewSession}>+ New session</button><nav className="session-list" aria-label="Chat sessions">{sessions.map((session) => <div className={activeSessionId === session.id ? "session-row active" : "session-row"} key={session.id}><button className={activeSessionId === session.id ? "session-item active" : "session-item"} type="button" onClick={() => onSelectSession(session.id)}>{session.title}</button><button className="session-delete-button" type="button" aria-label={`Delete ${session.title}`} onClick={() => onDeleteSession(session.id)}>×</button></div>)}{!sessions.length && <p className="sessions-empty">Your first message will create a session.</p>}</nav></aside>
    <section className="chat-panel">
      <header className="chat-header">
        <div className="chat-heading">
          <h1>{sessionTitle ?? "New session"}</h1>
          <p>{sessionTitle ? "Dummy assistant" : "Start a conversation"}</p>
        </div>
        <div className="chat-header-actions">
          <div
            className={`context-pie-widget ${isContextCritical ? "critical" : isContextWarning ? "warning" : ""}`}
            title={`Context usage: ${sessionTokenUsage.toLocaleString()} / ${activeContextLimit.toLocaleString()} tokens (${contextPercent}%)`}
          >
            <svg className="context-pie-svg" viewBox="0 0 36 36" aria-hidden="true">
              <path
                className="context-pie-bg"
                d="M18 2.0845 a 15.9155 15.9155 0 0 1 0 31.831 a 15.9155 15.9155 0 0 1 0 -31.831"
              />
              <path
                className="context-pie-fill"
                strokeDasharray={`${contextPercent}, 100`}
                d="M18 2.0845 a 15.9155 15.9155 0 0 1 0 31.831 a 15.9155 15.9155 0 0 1 0 -31.831"
              />
            </svg>
            <span className="context-pie-percent">{contextPercent}%</span>
          </div>
          {onCompact && (
            <button
              className="compact-button"
              type="button"
              onClick={onCompact}
              disabled={isSending || messages.length <= 1}
              title="Compact conversation into a structured status and task checkpoint"
            >
              <span className="compact-icon" aria-hidden="true">🗜️</span>
              <span>Compact</span>
            </button>
          )}
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
          {isSending && mode === "iterate" && (
            <div className="iterate-hud-banner">
              <div className="iterate-hud-info">
                <span className="iterate-hud-loop">Loop #{iterateStatus?.loop ?? 1}</span>
                <span className="iterate-hud-stepper">
                  <span className={`hud-step ${iterateStatus?.stage === "planner" ? "active" : "done"}`}>
                    📋 Planner
                  </span>
                  <span className="hud-arrow">➔</span>
                  <span className={`hud-step ${iterateStatus?.stage === "implementer" ? "active" : iterateStatus?.stage === "qa" ? "done" : ""}`}>
                    ⚙️ Implementer
                  </span>
                  <span className="hud-arrow">➔</span>
                  <span className={`hud-step ${iterateStatus?.stage === "qa" ? "active" : ""}`}>
                    🧪 QA Testing
                  </span>
                </span>
              </div>
              {isWrappingUp && (
                <span className="iterate-hud-pill">Wrapping up after QA…</span>
              )}
            </div>
          )}
          <textarea
            ref={textareaRef}
            className="composer-textarea"
            value={draft}
            onChange={(e: ChangeEvent<HTMLTextAreaElement>) => onDraftChange(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={mode === "iterate" ? "Describe your application goal to begin autonomous iteration… (Press Enter)" : mode === "max" ? "Describe the large application you want to build. Max will break it into tasks… (Press Enter)" : "Type a message… (Press Enter to send, Shift+Enter for new line)"}
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
              <div className={`composer-mode-picker ${mode === "plan" ? "plan-active" : mode === "qa" ? "qa-active" : mode === "iterate" ? "iterate-active" : mode === "max" ? "max-active" : ""}`}>
                <span className="composer-mode-icon">{mode === "plan" ? "📋" : mode === "qa" ? "🧪" : mode === "iterate" ? "🔄" : mode === "max" ? "🚀" : "⚙️"}</span>
                <span className="composer-mode-label">Mode:</span>
                <select
                  className="composer-mode-select"
                  value={mode}
                  aria-label="Mode Selection"
                  disabled={isSending}
                  onChange={({ target }) => onModeChange(target.value as "implement" | "plan" | "qa" | "iterate" | "max")}
                >
                  <option value="implement">Implement</option>
                  <option value="plan">Plan</option>
                  <option value="qa">QA / Test</option>
                  <option value="iterate">Iterate (Autonomous)</option>
                  <option value="max">Max (Large App)</option>
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
                        const currentReasoning = providers.find((p) => (p.id || p.key) === provider)?.reasoning ?? "off";
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
                        const providerToUse = activeProvider || (selectableProviders[0] ? (selectableProviders[0].id || selectableProviders[0].key) : "");
                        const modelToUse = activeModel || selectableProviders[0]?.models[0] || "";
                        setProviders((current) =>
                          current.map((p) => ((p.id || p.key) === providerToUse ? { ...p, reasoning: nextReasoning } : p))
                        );
                        if (providerToUse && modelToUse) {
                          onModelChange(providerToUse, modelToUse, nextReasoning);
                        }
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
                className={`composer-stop-btn ${mode === "iterate" ? (isWrappingUp ? "force-stop" : "wrap-up") : ""}`}
                type="button"
                onClick={onStop}
                aria-label={mode === "iterate" ? (isWrappingUp ? "Force Stop immediately" : "Wrap Up after QA") : "Stop generation"}
                title={mode === "iterate" ? (isWrappingUp ? "Force stop immediately" : "Wrap up: stops after current loop's QA") : "Stop"}
              >
                {mode === "iterate" ? (
                  isWrappingUp ? (
                    <>
                      <span className="composer-stop-icon" aria-hidden="true">⚡</span> Force Stop
                    </>
                  ) : (
                    <>
                      <span className="composer-stop-icon" aria-hidden="true">🛑</span> Wrap Up
                    </>
                  )
                ) : (
                  <>
                    <span className="composer-stop-icon" aria-hidden="true">■</span> Stop
                  </>
                )}
              </button>
            ) : (
              <button className="composer-send-btn" disabled={!draft.trim()} type="submit">
                {mode === "iterate" ? "Start Iteration" : "Send"}
              </button>
            )}
          </div>
        </div>
      </form>
    </section>
    <ReportMenu screenName="Project" />
    <SettingsPopup isOpen={isSettingsOpen} onClose={() => { setIsSettingsOpen(false); void refreshModels(); }} />
    {feedbackPopupTarget !== null && (
      <FeedbackPopup
        isOpen={true}
        onClose={() => setFeedbackPopupTarget(null)}
        activeProvider={activeProvider}
        activeModel={activeModel}
        messages={messages}
        targetMessageIndex={feedbackPopupTarget}
        onSubmitted={() => {
          if (feedbackPopupTarget !== null) {
            handleFeedbackSubmitted(feedbackPopupTarget);
          }
        }}
      />
    )}
  </main>;
}
