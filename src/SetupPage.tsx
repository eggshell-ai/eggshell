import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type DependencyStatus = { node: boolean; php: boolean; symfony: boolean; mysql: boolean };
type DependencyKey = keyof DependencyStatus;
type Dependency = { key: DependencyKey; name: string; version?: string };
type InstallOutcome = { installed: boolean; already_present: boolean; command: string; restart_required: boolean };
type InstallState = "idle" | "installing" | "failed";
type SetupState = { setup_completed: boolean; model: string };
type SetupStep = "dependencies" | "provider";
type Provider = { key: "ollama"; name: string; detail: string };

const dependencies: Dependency[] = [
  { key: "node", name: "Node JS", version: "24+" },
  { key: "php", name: "PHP", version: "8.2+" },
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
  const [isSaving, setIsSaving] = useState(false);

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

  async function setup() {
    setIsSettingUp(true); setFailures([]);
    const pending = dependencies
      .map((dependency, index) => ({ dependency, index }))
      .filter(({ index }) => !checked[index]);
    console.info("[SetupPage] Setup button clicked", { pending: pending.map(({ dependency }) => dependency.key) });

    const nextFailures: string[] = [];
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
      } catch (reason) {
        console.error("[SetupPage] install_dependency rejected", {
          dependency: dependency.key, reason, elapsedMs: Math.round(performance.now() - startedAt),
        });
        setInstallStates((current) => replaceAt(current, index, "failed"));
        nextFailures.push(`${dependency.name} could not be installed automatically. ${String(reason)}`);
      }
    }

    if (nextFailures.length > 0) {
      setFailures(nextFailures); setIsSettingUp(false);
      return;
    }
    console.info("[SetupPage] All installable dependencies ready; moving to provider setup");
    window.setTimeout(() => { setIsSettingUp(false); setStep("provider"); }, 450);
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

  if (step === "provider") return <main className="setup-page"><section className="setup-card" aria-labelledby="provider-title">
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
  </section></main>;

  return <main className="setup-page"><section className="setup-card" aria-labelledby="setup-title">
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
    <button className="setup-button" type="button" onClick={() => void setup()} disabled={isSettingUp || isDetecting}>
      {buttonLabel}
    </button>
    {restartNote}
  </section></main>;
}
