import { FormEvent, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import "./App.css";
import SetupPage from "./SetupPage";

type Project = { id: number; title: string; slug: string; path: string };
type ProjectForm = { title: string; slug: string; path: string };
type Message = { role: "user" | "assistant" | "thought" | "tool_call" | "tool_result"; content: string; data?: unknown };
type Session = { id: number; title: string; conversation_history: string };
const emptyProject: ProjectForm = { title: "", slug: "", path: "" };

function App() {
  const [isSetupComplete, setIsSetupComplete] = useState(false);
  const [projects, setProjects] = useState<Project[]>([]);
  const [isAdding, setIsAdding] = useState(false);
  const [form, setForm] = useState<ProjectForm>(emptyProject);
  const [error, setError] = useState("");
  const [isSaving, setIsSaving] = useState(false);
  const [activeProject, setActiveProject] = useState<Project | null>(null);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [activeSession, setActiveSession] = useState<Session | null>(null);
  const [draft, setDraft] = useState("");
  const [isSending, setIsSending] = useState(false);
  const [isStarting, setIsStarting] = useState(false);
  const [streamedMessages, setStreamedMessages] = useState<Message[]>([]);

  useEffect(() => { void loadProjects(); }, []);
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    void listen<{ projectId: number; event: { type: string; data: unknown } }>("agent-event", ({ payload }) => {
      if (payload.projectId !== activeProject?.id || payload.event.type === "complete") return;
      setStreamedMessages((current) => [...current, {
        role: payload.event.type as Message["role"],
        content: eventContent(payload.event.type, payload.event.data),
        data: payload.event.data,
      }]);
    }).then((stop) => { unlisten = stop; });
    return () => unlisten?.();
  }, [activeProject?.id]);

  async function loadProjects() {
    try { setProjects(await invoke<Project[]>("list_projects")); }
    catch (reason) { setError(String(reason)); }
  }

  async function chooseFolder() {
    const selection = await open({ directory: true, multiple: false, title: "Choose project folder" });
    if (typeof selection === "string") {
      setForm((current) => ({ ...current, path: selection, title: current.title || selection.split(/[\\/]/).filter(Boolean).pop() || "" }));
    }
  }

  async function addProject(event: FormEvent<HTMLFormElement>) {
    event.preventDefault(); setError(""); setIsSaving(true);
    try {
      const project = await invoke<Project>("create_project", { project: form });
      setProjects((current) => [...current, project].sort((a, b) => a.title.localeCompare(b.title)));
      setForm(emptyProject); setIsAdding(false);
    } catch (reason) { setError(String(reason)); }
    finally { setIsSaving(false); }
  }

  async function removeProject(project: Project) {
    if (!window.confirm(`Delete “${project.title}” from Eggshell? This does not delete its folder.`)) return;
    try { await invoke("delete_project", { id: project.id }); setProjects((current) => current.filter(({ id }) => id !== project.id)); }
    catch (reason) { setError(String(reason)); }
  }

  async function openProject(project: Project) {
    setError(""); setActiveProject(project); setActiveSession(null); setDraft(""); setIsStarting(false);
    try { setSessions(await invoke<Session[]>("list_sessions", { projectId: project.id })); }
    catch (reason) { setError(String(reason)); }
  }
  async function removeSession(session: Session) {
    if (!activeProject || !window.confirm(`Delete “${session.title}”? This cannot be undone.`)) return;
    try {
      await invoke("delete_session", { projectId: activeProject.id, id: session.id });
      setSessions((current) => current.filter(({ id }) => id !== session.id));
      if (activeSession?.id === session.id) setActiveSession(null);
    } catch (reason) { setError(String(reason)); }
  }
  function startNewSession() { setActiveSession(null); setDraft(""); }
  async function startProject() {
    if (!activeProject || isStarting) return;
    setError(""); setIsStarting(true);
    try {
      await invoke("start_project", { id: activeProject.id });
      await new Promise((resolve) => window.setTimeout(resolve, 3000));
    } catch (reason) { setError(String(reason)); }
    finally { setIsStarting(false); }
  }
  async function sendMessage(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!activeProject || !draft.trim() || isSending) return;
    setError(""); setIsSending(true); setStreamedMessages([]);
    try {
      const session = await invoke<Session>("send_message", { projectId: activeProject.id, sessionId: activeSession?.id ?? null, message: draft });
      setActiveSession(session); setSessions((current) => [session, ...current.filter(({ id }) => id !== session.id)]); setDraft("");
    } catch (reason) { setError(String(reason)); }
    finally { setIsSending(false); }
  }
  const persistedMessages: Message[] = activeSession ? JSON.parse(activeSession.conversation_history) : [];
  const messages: Message[] = isSending
    ? [...persistedMessages, { role: "user", content: draft }, ...streamedMessages]
    : persistedMessages;
  if (!isSetupComplete) return <SetupPage onComplete={() => setIsSetupComplete(true)} />;
  if (activeProject) return <main className="chat-layout">
    <aside className="chat-sidebar"><button className="back-button" type="button" onClick={() => { setIsStarting(false); setActiveProject(null); }}>← Projects</button><div className="project-name"><p className="eyebrow">Project</p><h2>{activeProject.title}</h2></div><button className="start-button" type="button" onClick={() => void startProject()} disabled={isStarting}>{isStarting ? "Starting…" : "Start"}</button><button className="new-chat-button" type="button" onClick={startNewSession}>+ New session</button><nav className="session-list" aria-label="Chat sessions">{sessions.map((session) => <div className={activeSession?.id === session.id ? "session-row active" : "session-row"} key={session.id}><button className="session-item" type="button" onClick={() => setActiveSession(session)}>{session.title}</button><button className="session-delete-button" type="button" aria-label={`Delete ${session.title}`} onClick={() => void removeSession(session)}>×</button></div>)}{!sessions.length && <p className="sessions-empty">Your first message will create a session.</p>}</nav></aside>
    <section className="chat-panel"><header className="chat-header"><h1>{activeSession?.title ?? "New session"}</h1><p>{activeSession ? "Dummy assistant" : "Start a conversation"}</p></header><div className="message-list" aria-live="polite">{!messages.length && <div className="chat-empty"><h2>How can I help?</h2><p>Send a message to begin.</p></div>}{messages.map((message, index) => <article className={`message ${message.role}`} key={`${message.role}-${index}`}><span>{message.role === "user" ? "You" : "Eggshell"}</span><p>{message.content}</p></article>)}</div>{error && <p className="chat-error" role="alert">{error}</p>}<form className="composer" onSubmit={sendMessage}><input value={draft} onChange={(event) => setDraft(event.target.value)} disabled={isSending} placeholder="Message Eggshell…" aria-label="Message" /><button className="add-button" disabled={isSending || !draft.trim()} type="submit">{isSending ? "Sending…" : "Send"}</button></form></section>
  </main>;

  return (
    <main className="home">
      <header className="page-header"><div><p className="eyebrow">Eggshell</p><h1>Your projects</h1><p className="subtitle">Keep the local projects you work on close at hand.</p></div><button className="add-button" type="button" onClick={() => { setError(""); setIsAdding(true); }}><span aria-hidden="true">+</span> Add project</button></header>
      {error && <p className="error" role="alert">{error}</p>}
      {isAdding && <section className="dialog-backdrop" role="presentation"><form className="project-dialog" onSubmit={addProject}>
        <div className="dialog-heading"><div><p className="eyebrow">New project</p><h2>Add a local project</h2></div><button className="icon-button" type="button" aria-label="Close" onClick={() => setIsAdding(false)}>×</button></div>
        <label>Project folder<div className="folder-picker"><input value={form.path} readOnly placeholder="Select a folder" /><button type="button" onClick={chooseFolder}>Browse</button></div></label>
        <label>Title<input required value={form.title} onChange={(event) => setForm({ ...form, title: event.target.value })} placeholder="My project" /></label>
        <label>Slug <span>Optional</span><input value={form.slug} onChange={(event) => setForm({ ...form, slug: event.target.value })} placeholder="Generated from the title" /></label>
        <div className="dialog-actions"><button className="secondary-button" type="button" onClick={() => setIsAdding(false)}>Cancel</button><button className="add-button" disabled={isSaving || !form.path} type="submit">{isSaving ? "Adding…" : "Add project"}</button></div>
      </form></section>}
      <section className="projects" aria-label="Projects">
        {projects.map((project) => <article className="project-tile" key={project.id} onClick={() => void openProject(project)}><div className="project-mark" aria-hidden="true">{project.title.slice(0, 1).toUpperCase()}</div><div className="project-info"><h2>{project.title}</h2><p>/{project.slug}</p><span title={project.path}>{project.path}</span></div><button className="delete-button" type="button" onClick={(event) => { event.stopPropagation(); void removeProject(project); }}>Delete</button></article>)}
        {!projects.length && <div className="empty-state">No projects yet. Add one to get started.</div>}
      </section>
    </main>
  );
}

function eventContent(type: string, data: unknown) {
  const eventData = data as { content?: string; name?: string; arguments?: unknown; result?: unknown };
  if (type === "thought") return eventData.content ?? "";
  if (type === "tool_call") return `${eventData.name ?? "tool"}(${JSON.stringify(eventData.arguments ?? {})})`;
  if (type === "tool_result") return `${eventData.name ?? "tool"} returned ${JSON.stringify(eventData.result ?? {})}`;
  return JSON.stringify(data);
}

export default App;
