import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Dependency = { name: string; version?: string };
const dependencies: Dependency[] = [
  { name: "Node JS", version: "24+" }, { name: "PHP", version: "8.2+" },
  { name: "Symfony CLI" }, { name: "MySQL" },
];
type DependencyStatus = { node: boolean; php: boolean; symfony: boolean; mysql: boolean };
const statusKeys: (keyof DependencyStatus)[] = ["node", "php", "symfony", "mysql"];
type SetupPageProps = { onComplete: () => void };

export default function SetupPage({ onComplete }: SetupPageProps) {
  const [isSettingUp, setIsSettingUp] = useState(false);
  const [isDetecting, setIsDetecting] = useState(true);
  const [checked, setChecked] = useState<boolean[]>(dependencies.map(() => false));

  useEffect(() => {
    const startedAt = performance.now();
    console.info("[SetupPage] Dependency detection started", { startedAt: new Date().toISOString() });

    void invoke<DependencyStatus>("detect_dependencies")
      .then((status) => {
        const nextChecked = statusKeys.map((key) => status[key]);
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

  function setup() {
    console.info("[SetupPage] Setup button clicked; starting completion timer");
    setIsSettingUp(true);
    window.setTimeout(() => {
      console.info("[SetupPage] Completion timer fired; calling onComplete");
      onComplete();
    }, 450);
  }

  return <main className="setup-page"><section className="setup-card" aria-labelledby="setup-title">
    <div className="setup-brand" aria-hidden="true">e</div><p className="eyebrow">Welcome to Eggshell</p>
    <h1 id="setup-title">Let’s get you set up</h1>
    <div className="dependency-list" aria-label="Required dependencies">
      {dependencies.map((dependency, index) => <div className="dependency-row" key={dependency.name}>
        <span>{dependency.name}{dependency.version && <small>{dependency.version}</small>}</span>
        <span className={checked[index] ? "dependency-check complete" : "dependency-check"} aria-label={checked[index] ? "Ready" : "Not found"}>{checked[index] ? "✓" : ""}</span>
      </div>)}
    </div>
    <button className="setup-button" type="button" onClick={setup} disabled={isSettingUp || isDetecting}>
      {isDetecting ? "Checking dependencies…" : isSettingUp ? "Setting up…" : "Setup"}
    </button>
  </section></main>;
}
