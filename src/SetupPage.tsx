import { useState } from "react";

type Dependency = {
  name: string;
  version?: string;
};

const dependencies: Dependency[] = [
  { name: "Node JS", version: "24+" },
  { name: "PHP", version: "8.2+" },
  { name: "Symfony CLI" },
  { name: "MySQL" },
];

type SetupPageProps = {
  onComplete: () => void;
};

export default function SetupPage({ onComplete }: SetupPageProps) {
  const [isSettingUp, setIsSettingUp] = useState(false);
  const [checked, setChecked] = useState<boolean[]>(dependencies.map(() => false));

  function setup() {
    setIsSettingUp(true);
    setChecked(dependencies.map(() => true));
    window.setTimeout(onComplete, 450);
  }

  return (
    <main className="setup-page">
      <section className="setup-card" aria-labelledby="setup-title">
        <div className="setup-brand" aria-hidden="true">e</div>
        <p className="eyebrow">Welcome to Eggshell</p>
        <h1 id="setup-title">Let’s get you set up</h1>

        <div className="dependency-list" aria-label="Required dependencies">
          {dependencies.map((dependency, index) => (
            <div className="dependency-row" key={dependency.name}>
              <span>{dependency.name}{dependency.version && <small>{dependency.version}</small>}</span>
              <span className={checked[index] ? "dependency-check complete" : "dependency-check"} aria-label={checked[index] ? "Ready" : "Not checked"}>
                {checked[index] ? "✓" : ""}
              </span>
            </div>
          ))}
        </div>

        <button className="setup-button" type="button" onClick={setup} disabled={isSettingUp}>
          {isSettingUp ? "Setting up…" : "Setup"}
        </button>
      </section>
    </main>
  );
}
