import { FormEvent, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import "./App.css";
import SetupPage from "./SetupPage";
import ProjectLogPanel, { ProgressLine } from "./ProjectLogPanel";
import Chat, { ChatMessage, eventContent } from "./Chat";

type Project = { id: number; title: string; slug: string; path: string };
type ProjectForm = { title: string; slug: string; path: string };
type Session = { id: number; title: string; conversation_history: string };
type SetupState = { setup_completed: boolean; model: string };
const emptyProject: ProjectForm = { title: "", slug: "", path: "" };

function App() {
  // `null` until config.yaml has been read, so a completed setup never flashes
  // the setup screen on the way past it.
  const [isSetupComplete, setIsSetupComplete] = useState<boolean | null>(null);
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
  const [streamedMessages, setStreamedMessages] = useState<ChatMessage[]>([]);
  // What the shells have reported for the project currently being created.
  const [createLog, setCreateLog] = useState<ProgressLine[]>([]);

  useEffect(() => { void loadProjects(); }, []);
  useEffect(() => {
    void invoke<SetupState>("load_setup_state")
      .then(({ setup_completed }) => setIsSetupComplete(setup_completed))
      .catch((reason: unknown) => {
        console.error("[App] load_setup_state rejected", { reason });
        setIsSetupComplete(false);
      });
  }, []);
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    void listen<{ projectId: number; event: { type: string; data: unknown } }>("agent-event", ({ payload }) => {
      if (payload.projectId !== activeProject?.id || payload.event.type === "complete") return;
      setStreamedMessages((current) => [...current, {
        role: payload.event.type as ChatMessage["role"],
        content: eventContent(payload.event.type, payload.event.data),
        data: payload.event.data,
      }]);
    }).then((stop) => { unlisten = stop; });
    return () => unlisten?.();
  }, [activeProject?.id]);
  // Registered at mount rather than when a create starts: `listen` resolves
  // asynchronously, and the first commands would have run before it was ready.
  // That is also why this needs no history command, unlike the setup log.
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let stopped = false;
    void listen<ProgressLine>("project-log", ({ payload }) => {
      setCreateLog((current) => current.some(({ seq }) => seq === payload.seq) ? current : [...current, payload]);
    }).then((stop) => { if (stopped) stop(); else unlisten = stop; })
      .catch((reason: unknown) => console.error("[App] project log unavailable", { reason }));
    return () => { stopped = true; unlisten?.(); };
  }, []);

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
    // The backend builds a fresh log per creation, so its sequence numbers restart
    // at 1 — anything left over from an earlier attempt would swallow the new lines.
    setCreateLog([]);
    try {
      const project = await invoke<Project>("create_project", { project: form });
      setProjects((current) => [...current, project].sort((a, b) => a.title.localeCompare(b.title)));
      setForm(emptyProject); setIsAdding(false); setCreateLog([]);
    } catch (reason) { setError(String(reason)); }
    finally { setIsSaving(false); }
  }

  function closeAddProject() { setIsAdding(false); setCreateLog([]); setError(""); }

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
  const persistedMessages: ChatMessage[] = activeSession ? JSON.parse(activeSession.conversation_history) : [];
  const messages: ChatMessage[] = isSending
    ? [...persistedMessages, { role: "user", content: draft }, ...streamedMessages]
    : persistedMessages;
  // The log replaces the form while the shells run, and stays put afterwards when
  // they failed — the dialog is the only thing that can set an error while it is
  // open, so the lines and the reason belong together.
  const showCreateLog = isSaving || createLog.length > 0;
  if (isSetupComplete === null) return null; // loading config
  if (!isSetupComplete) return <SetupPage onComplete={() => setIsSetupComplete(true)} />;
  if (activeProject) return <Chat projectTitle={activeProject.title} sessionTitle={activeSession?.title} sessions={sessions} activeSessionId={activeSession?.id} messages={messages} draft={draft} isSending={isSending} isStarting={isStarting} error={error} onBack={() => { setIsStarting(false); setActiveProject(null); }} onStart={() => void startProject()} onNewSession={startNewSession} onSelectSession={(id) => setActiveSession(sessions.find((session) => session.id === id) ?? null)} onDeleteSession={(id) => { const session = sessions.find((item) => item.id === id); if (session) void removeSession(session); }} onDraftChange={setDraft} onSend={sendMessage} />;

  return (
    <main className="home">
      <header className="page-header"><div><p className="eyebrow">Eggshell</p><h1>Your projects</h1><p className="subtitle">Keep the local projects you work on close at hand.</p></div><button className="add-button" type="button" onClick={() => { setError(""); setIsAdding(true); }}><span aria-hidden="true">+</span> Add project</button></header>
      {error && !isAdding && <p className="error" role="alert">{error}</p>}
      {isAdding && <section className="dialog-backdrop" role="presentation"><form className={showCreateLog ? "project-dialog creating" : "project-dialog"} onSubmit={addProject}>
        <div className="dialog-heading"><div><p className="eyebrow">New project</p><h2>{showCreateLog ? (isSaving ? `Setting up ${form.title || "your project"}…` : "Setup did not finish") : "Add a local project"}</h2></div>{!isSaving && <button className="icon-button" type="button" aria-label="Close" onClick={closeAddProject}>×</button>}</div>
        {showCreateLog
          ? <>
            <p className="dialog-note">{isSaving ? "Installing dependencies and preparing the database. This takes a few minutes." : "The output below shows how far it got."}</p>
            <ProjectLogPanel lines={createLog} />
            {error && <p className="dialog-error" role="alert">{error}</p>}
            {!isSaving && <div className="dialog-actions"><button className="secondary-button" type="button" onClick={() => { setCreateLog([]); setError(""); }}>Try again</button><button className="add-button" type="button" onClick={closeAddProject}>Close</button></div>}
          </>
          : <>
            <label>Project folder<div className="folder-picker"><input value={form.path} readOnly placeholder="Select a folder" /><button type="button" onClick={chooseFolder}>Browse</button></div></label>
            <label>Title<input required value={form.title} onChange={(event) => setForm({ ...form, title: event.target.value })} placeholder="My project" /></label>
            <label>Slug <span>Optional</span><input value={form.slug} onChange={(event) => setForm({ ...form, slug: event.target.value })} placeholder="Generated from the title" /></label>
            {error && <p className="dialog-error" role="alert">{error}</p>}
            <div className="dialog-actions"><button className="secondary-button" type="button" onClick={closeAddProject}>Cancel</button><button className="add-button" disabled={isSaving || !form.path} type="submit">{isSaving ? "Adding…" : "Add project"}</button></div>
          </>}
      </form></section>}
      <section className="projects" aria-label="Projects">
        {projects.map((project) => <article className="project-tile" key={project.id} onClick={() => void openProject(project)}><div className="project-mark" aria-hidden="true">{project.title.slice(0, 1).toUpperCase()}</div><div className="project-info"><h2>{project.title}</h2><p>/{project.slug}</p><span title={project.path}>{project.path}</span></div><button className="delete-button" type="button" onClick={(event) => { event.stopPropagation(); void removeProject(project); }}>Delete</button></article>)}
        {!projects.length && <div className="empty-state">No projects yet. Add one to get started.</div>}
      </section>
    </main>
  );
}

export default App;
