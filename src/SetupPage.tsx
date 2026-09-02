import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { message } from "@tauri-apps/plugin-dialog";
import ReportMenu from "./ReportMenu";

type DependencyStatus = { node: boolean; php: boolean; composer: boolean; symfony: boolean; mysql: boolean };
type DependencyKey = keyof DependencyStatus;
type Dependency = { key: DependencyKey; name: string; version?: string };
type InstallOutcome = { installed: boolean; already_present: boolean; command: string; restart_required: boolean };
type InstallState = "idle" | "installing" | "failed";
type SetupState = { setup_completed: boolean; model: string };
type SetupStep = "dependencies" | "provider";
type Provider = { key: "ollama"; name: string; detail: string };
// Mirrors `setup::LogLine`: `stream` is what produced the line, and the panel colours by it.
type LogLine = { seq: number; stream: "info" | "command" | "stdout" | "stderr" | "error"; text: string };
// Mirrors `logger::LogEntry`: the central log the backend accumulates for reporting.
type LogEntry = { level: "info" | "warning" | "error"; message: string; sensitive: boolean };

const dependencies: Dependency[] = [
  { key: "node", name: "Node JS", version: "24+" },
  { key: "php", name: "PHP", version: "8.2+" },
  { key: "composer", name: "Composer" },
  { key: "symfony", name: "Symfony CLI" },
  { key: "mysql", name: "MySQL" },
];
const providers: Provider[] = [
  { key: "ollama", name: "Ollama", detail: "Cloud and local models from ollama.com" },
];
type SetupPageProps = { onComplete: () => void };

function replaceAt<T>(values: T[], index: number, value: T): T[] {
  return values.map((current, position) => (position === index ? value : current));
}

export default function SetupPage({ onComplete }: SetupPageProps) {
  const [step, setStep] = useState<SetupStep>("dependencies");
  const [isSettingUp, setIsSettingUp] = useState(false);
  const [isDetecting, setIsDetecting] = useState(true);
  const [checked, setChecked] = useState<boolean[]>(dependencies.map(() => false));
  const [installStates, setInstallStates] = useState<InstallState[]>(dependencies.map(() => "idle"));
  const [failures, setFailures] = useState<string[]>([]);
  const [needsRestart, setNeedsRestart] = useState(false);
  const [provider, setProvider] = useState<Provider["key"] | null>(null);
  const [model, setModel] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [mysqlPassword, setMysqlPassword] = useState("");
  const [showMysqlPassword, setShowMysqlPassword] = useState(false);
  const [isSavingMysql, setIsSavingMysql] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [logLines, setLogLines] = useState<LogLine[]>([]);
  const [isLogOpen, setIsLogOpen] = useState(false);
  const [accumulatedLogs, setAccumulatedLogs] = useState<LogEntry[]>([]);
  const logRef = useRef<HTMLDivElement | null>(null);
  // Following the output is only welcome while the user is already at the bottom;
  // yanking the view away from a line they scrolled back to read is not.
  const isPinnedRef = useRef(true);

  // Every line carries a sequence number, so history fetched after a live event
  // still lands in order and a line already on screen is never repeated.
  function appendLine(line: LogLine) {
    setLogLines((current) => {
      if (current.some(({ seq }) => seq === line.seq)) return current;
      const position = current.findIndex(({ seq }) => seq > line.seq);
      if (position === -1) return [...current, line];
      return [...current.slice(0, position), line, ...current.slice(position)];
    });
  }

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let stopped = false;

    void listen<LogLine>("setup-log", ({ payload }) => appendLine(payload))
      .then((stop) => { if (stopped) stop(); else unlisten = stop; })
      // Dependency detection and MySQL's start-up have already logged by the time
      // this mounts, and that is exactly what someone opening the log wants first.
      .then(() => invoke<LogLine[]>("setup_log_history"))
      .then((history) => history.forEach(appendLine))
      .catch((error: unknown) => console.error("[SetupPage] setup log unavailable", { error }));

    return () => { stopped = true; unlisten?.(); };
  }, []);

  useEffect(() => {
    const panel = logRef.current;
    if (panel && isPinnedRef.current) panel.scrollTop = panel.scrollHeight;
  }, [logLines, isLogOpen]);

  useEffect(() => {
    const startedAt = performance.now();
    console.info("[SetupPage] Dependency detection started", { startedAt: new Date().toISOString() });

    void invoke<DependencyStatus>("detect_dependencies")
      .then((status) => {
        const nextChecked = dependencies.map(({ key }) => status[key]);
        setChecked(nextChecked);
      })
      .catch((error: unknown) => {
        console.error("[SetupPage] detect_dependencies rejected", {
          error,
          elapsedMs: Math.round(performance.now() - startedAt),
        });
        setChecked(dependencies.map(() => false));
      })
      .finally(() => {
        console.info("[SetupPage] Dependency detection finished; clearing loading state", {
          elapsedMs: Math.round(performance.now() - startedAt),
        });
        setIsDetecting(false);
      });
  }, []);

  // Whatever model config.yaml already names is the best first suggestion.
  useEffect(() => {
    void invoke<SetupState>("load_setup_state")
      .then(({ model: configured }) => setModel((current) => current || configured))
      .catch((error: unknown) => console.error("[SetupPage] load_setup_state rejected", { error }));
  }, []);

  // The central logger accumulates everything the backend has recorded, so the
  // report menu can quote it. Refreshed whenever the log panel is opened, which
  // is roughly when a person having trouble would go looking.
  useEffect(() => {
    if (!isLogOpen) return;
    void invoke<LogEntry[]>("read_logs")
      .then(setAccumulatedLogs)
      .catch((error: unknown) => console.error("[SetupPage] read_logs rejected", { error }));
  }, [isLogOpen, logLines.length]);

  async function setup() {
    setIsSettingUp(true); setFailures([]);
    const pending = dependencies
      .map((dependency, index) => ({ dependency, index }))
      .filter(({ index }) => !checked[index]);
    const mysqlIndex = dependencies.findIndex(({ key }) => key === "mysql");
    const mysqlAlreadyDetected = checked[mysqlIndex] === true;
    console.info("[SetupPage] Setup button clicked", { pending: pending.map(({ dependency }) => dependency.key) });

    const nextFailures: string[] = [];
    let mysqlPasswordRequired = false;
    for (const { dependency, index } of pending) {
      const startedAt = performance.now();
      console.info("[SetupPage] install_dependency started", { dependency: dependency.key });
      setInstallStates((current) => replaceAt(current, index, "installing"));
      try {
        const outcome = await invoke<InstallOutcome>("install_dependency", { name: dependency.key });
        console.info("[SetupPage] install_dependency resolved", {
          dependency: dependency.key, outcome, elapsedMs: Math.round(performance.now() - startedAt),
        });
        setChecked((current) => replaceAt(current, index, true));
        setInstallStates((current) => replaceAt(current, index, "idle"));
        if (outcome.restart_required) setNeedsRestart(true);
        if (dependency.key === "mysql" && (outcome.already_present || outcome.command.startsWith("winget "))) {
          const installedByEggshell = !outcome.already_present && outcome.command.startsWith("winget ");
          await message(installedByEggshell
            ? "MySQL has been installed with winget. Launch and configure the service manually, then enter the root password below. Leave it blank if root has no password."
            : "MySQL is already installed on this computer. Make sure its service is running, then enter the root password below. Leave it blank if root has no password.", {
            title: "MySQL setup warning",
            kind: "warning",
          });
          setShowMysqlPassword(true);
          mysqlPasswordRequired = true;
        }
      } catch (reason) {
        console.error("[SetupPage] install_dependency rejected", {
          dependency: dependency.key, reason, elapsedMs: Math.round(performance.now() - startedAt),
        });
        setInstallStates((current) => replaceAt(current, index, "failed"));
        // A failure is the one moment the output is worth more than the summary,
        // so the log opens itself rather than waiting to be asked.
        setIsLogOpen(true);
        nextFailures.push(`${dependency.name} could not be installed automatically. ${String(reason)}`);
      }
    }

    if (nextFailures.length > 0) {
      setFailures(nextFailures); setIsSettingUp(false);
      return;
    }
    if (mysqlAlreadyDetected) {
      await message(
        "MySQL is already installed on this computer. Make sure its service is running, then enter the root password below. Leave it blank if root has no password.",
        { title: "MySQL setup warning", kind: "warning" },
      );
      setShowMysqlPassword(true);
      setIsSettingUp(false);
      return;
    }
    if (mysqlPasswordRequired) {
      setIsSettingUp(false);
      return;
    }
    console.info("[SetupPage] All installable dependencies ready; moving to provider setup");
    window.setTimeout(() => { setIsSettingUp(false); setStep("provider"); }, 450);
  }

  async function saveMysqlPassword() {
    setIsSavingMysql(true); setFailures([]);
    try {
      await invoke("save_mysql_config", { password: mysqlPassword });
      setShowMysqlPassword(false);
      setStep("provider");
    } catch (reason) {
      setFailures([`The MySQL settings could not be saved. ${String(reason)}`]);
    } finally { setIsSavingMysql(false); }
  }

  async function saveProvider() {
    if (!provider || !model.trim() || !apiKey.trim()) return;
    setIsSaving(true); setFailures([]);
    console.info("[SetupPage] save_provider_config started", { provider, model });
    try {
      await invoke("save_provider_config", { provider, model, apiKey });
      console.info("[SetupPage] save_provider_config resolved; calling onComplete");
      onComplete();
    } catch (reason) {
      console.error("[SetupPage] save_provider_config rejected", { reason });
      setFailures([`Your provider settings could not be saved. ${String(reason)}`]);
      setIsSaving(false);
    }
  }

  const buttonLabel = isDetecting ? "Checking dependencies…"
    : isSettingUp ? "Installing…"
    : failures.length > 0 ? "Retry setup"
    : "Setup";

  const restartNote = needsRestart
    ? <p className="setup-note">Restart Eggshell so it picks up the newly installed tools.</p>
    : null;

  // One control on either step: a toggle, and the panel it reveals. Collapsed, the
  // line count is the only hint that anything is being recorded.
  const logSection = <>
    <button className="log-toggle" type="button" aria-expanded={isLogOpen}
      onClick={() => setIsLogOpen((open) => !open)}>
      {isLogOpen ? "Hide log" : "View log"}
      {!isLogOpen && logLines.length > 0 && <span> · {logLines.length} lines</span>}
    </button>
    {isLogOpen && <div
      className="setup-log" role="log" aria-label="Setup log" ref={logRef}
      onScroll={({ currentTarget }) => {
        const { scrollTop, scrollHeight, clientHeight } = currentTarget;
        isPinnedRef.current = scrollHeight - scrollTop - clientHeight < 24;
      }}
    >
      {logLines.length === 0
        ? <p className="log-line info">Nothing has run yet.</p>
        : logLines.map(({ seq, stream, text }) => <p className={`log-line ${stream}`} key={seq}>{text}</p>)}
    </div>}
  </>;

  const cardClass = isLogOpen ? "setup-card with-log" : "setup-card";

  if (step === "provider") return <main className="setup-page"><section className={cardClass} aria-labelledby="provider-title">
    <div className="setup-brand" aria-hidden="true">e</div><p className="eyebrow">Final step</p>
    <h1 id="provider-title">Configure Your LLM Provider</h1>
    <p className="setup-intro">Pick the provider Eggshell should send your prompts to.</p>
    <div className="provider-list" aria-label="Providers">
      {providers.map(({ key, name, detail }) => <button
        className={provider === key ? "provider-tile selected" : "provider-tile"}
        key={key} type="button" aria-pressed={provider === key} onClick={() => setProvider(key)}
      >
        <span className="provider-mark" aria-hidden="true">{name.slice(0, 1)}</span>
        <span className="provider-copy"><strong>{name}</strong><small>{detail}</small></span>
      </button>)}
    </div>
    {provider === "ollama" && <div className="provider-form">
      <label>API key<input value={apiKey} type="password" autoComplete="off" spellCheck={false}
        onChange={(event) => setApiKey(event.target.value)} placeholder="Paste your Ollama API key" /></label>
      <label>Model<input value={model} autoComplete="off" spellCheck={false}
        onChange={(event) => setModel(event.target.value)} placeholder="gemma4:31b-cloud" /></label>
    </div>}
    {failures.length > 0 && <div className="setup-error" role="alert">
      {failures.map((failure) => <p key={failure}>{failure}</p>)}
    </div>}
    <button className="setup-button" type="button" onClick={() => void saveProvider()}
      disabled={isSaving || !provider || !model.trim() || !apiKey.trim()}>
      {isSaving ? "Saving…" : "Next"}
    </button>
    {restartNote}
    {logSection}
    <ReportMenu screenName="provider" accumulatedLogs={accumulatedLogs} />
  </section></main>;

  return <main className="setup-page"><section className={cardClass} aria-labelledby="setup-title">
    <div className="setup-brand" aria-hidden="true">e</div><p className="eyebrow">Welcome to Eggshell</p>
    <h1 id="setup-title">Let’s get you set up</h1>
    <div className="dependency-list" aria-label="Required dependencies">
      {dependencies.map((dependency, index) => <div className="dependency-row" key={dependency.key}>
        <span>{dependency.name}{dependency.version && <small>{dependency.version}</small>}</span>
        {installStates[index] === "installing"
          ? <small className="dependency-status" role="status">Installing…</small>
          : <span
              className={checked[index] ? "dependency-check complete" : installStates[index] === "failed" ? "dependency-check failed" : "dependency-check"}
              aria-label={checked[index] ? "Ready" : installStates[index] === "failed" ? "Installation failed" : "Not found"}
            >{checked[index] ? "✓" : installStates[index] === "failed" ? "!" : ""}</span>}
      </div>)}
    </div>
    {failures.length > 0 && <div className="setup-error" role="alert">
      {failures.map((failure) => <p key={failure}>{failure}</p>)}
    </div>}
    {showMysqlPassword && <div className="provider-form" role="dialog" aria-labelledby="mysql-password-title">
      <h2 id="mysql-password-title">MySQL root password</h2>
      <p className="setup-intro">Enter the password configured for the MySQL <code>root</code> user.</p>
      <label>Password<input value={mysqlPassword} type="password" autoFocus autoComplete="off"
        onChange={(event) => setMysqlPassword(event.target.value)} /></label>
      <button className="setup-button" type="button" onClick={() => void saveMysqlPassword()} disabled={isSavingMysql}>
        {isSavingMysql ? "Saving…" : "Continue"}
      </button>
    </div>}
    <button className="setup-button" type="button" onClick={() => void setup()} disabled={isSettingUp || isDetecting || showMysqlPassword}>
      {buttonLabel}
    </button>
    {restartNote}
    {logSection}
    <ReportMenu screenName="Setup" accumulatedLogs={accumulatedLogs} />
  </section></main>;
}
