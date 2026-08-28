import { FormEvent } from "react";

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

function formatToolMessage(event: ToolEvent, type: "tool_call" | "tool_result") {
  const name = event.name ?? "tool";
  if (type === "tool_call") return toolCallFormatters[name]?.(event) ?? `Using tool: ${name}`;
  return `Completed tool: ${name}`;
}

export function eventContent(type: string, data: unknown) {
  const event = data as ToolEvent & { content?: string };
  if (type === "thought") return event.content ?? "";
  if (type === "tool_call" || type === "tool_result") return formatToolMessage(event, type);
  return JSON.stringify(data);
}

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
};

export default function Chat({ projectTitle, sessionTitle, sessions, activeSessionId, messages, draft, isSending, isStarting, error, onBack, onStart, onNewSession, onSelectSession, onDeleteSession, onDraftChange, onSend }: ChatProps) {
  return <main className="chat-layout">
    <aside className="chat-sidebar"><button className="back-button" type="button" onClick={onBack}>← Projects</button><div className="project-name"><p className="eyebrow">Project</p><h2>{projectTitle}</h2></div><button className="start-button" type="button" onClick={onStart} disabled={isStarting}>{isStarting ? "Starting…" : "Start"}</button><button className="new-chat-button" type="button" onClick={onNewSession}>+ New session</button><nav className="session-list" aria-label="Chat sessions">{sessions.map((session) => <div className={activeSessionId === session.id ? "session-row active" : "session-row"} key={session.id}><button className={activeSessionId === session.id ? "session-item active" : "session-item"} type="button" onClick={() => onSelectSession(session.id)}>{session.title}</button><button className="session-delete-button" type="button" aria-label={`Delete ${session.title}`} onClick={() => onDeleteSession(session.id)}>×</button></div>)}{!sessions.length && <p className="sessions-empty">Your first message will create a session.</p>}</nav></aside>
    <section className="chat-panel"><header className="chat-header"><h1>{sessionTitle ?? "New session"}</h1><p>{sessionTitle ? "Dummy assistant" : "Start a conversation"}</p></header><div className="message-list" aria-live="polite">{!messages.length && <div className="chat-empty"><h2>How can I help?</h2><p>Send a message to begin.</p></div>}{messages.map((message, index) => <article className={`message ${message.role}`} key={`${message.role}-${index}`}><span>{message.role === "user" ? "You" : "Eggshell"}</span><p>{message.data && (message.role === "tool_call" || message.role === "tool_result") ? formatToolMessage(message.data as ToolEvent, message.role) : message.content}</p></article>)}</div>{error && <p className="chat-error" role="alert">{error}</p>}<form className="composer" onSubmit={onSend}><input value={draft} onChange={(event) => onDraftChange(event.target.value)} disabled={isSending} placeholder="Message Eggshell…" aria-label="Message" /><button className="add-button" disabled={isSending || !draft.trim()} type="submit">{isSending ? "Sending…" : "Send"}</button></form></section>
  </main>;
}
