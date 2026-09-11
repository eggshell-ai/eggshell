import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type RegisteredProvider = { key: string; name: string; detail: string; api_key_set: boolean; models: string[] };
type SetupState = { setup_completed: boolean; providers: RegisteredProvider[] };

type SettingsPopupProps = { isOpen: boolean; onClose: () => void };

// Add an entry here (and a matching panel in the body) to give the settings
// sidebar another section. Only providers exist today.
type SettingsSectionKey = "providers";
const settingsSections: { key: SettingsSectionKey; label: string; detail: string }[] = [
  { key: "providers", label: "Providers", detail: "Models and API keys" },
];

// The provider rows are a sidebar of their own: the key identifies the provider
// the editor on the right is pointed at, and `null` means "compose a new one".
function describeModels(models: string[]) {
  if (!models.length) return "No models yet";
  return models.length === 1 ? models[0] : `${models.length} models`;
}

export default function SettingsPopup({ isOpen, onClose }: SettingsPopupProps) {
  const [providers, setProviders] = useState<RegisteredProvider[]>([]);
  const [loading, setLoading] = useState(false);
  const [section, setSection] = useState<SettingsSectionKey>("providers");
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [newType, setNewType] = useState("");
  const [models, setModels] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [error, setError] = useState("");
  const [isSaving, setIsSaving] = useState(false);

  const selected = providers.find(({ key }) => key === selectedKey) ?? null;
  const isNew = selectedKey === null;

  function startNewProvider() {
    setSelectedKey(null); setNewType(""); setModels(""); setApiKey(""); setError("");
  }

  // Opening the popup lands on an empty editor, so adding a provider is a
  // single click rather than a "Add New" detour.
  useEffect(() => {
    if (!isOpen) return;
    setSection("providers");
    startNewProvider();
    setLoading(true);
    void invoke<SetupState>("load_setup_state")
      .then(({ providers: loaded }) => setProviders(loaded))
      .catch((reason: unknown) => console.error("[SettingsPopup] load_setup_state rejected", { reason }))
      .finally(() => setLoading(false));
  }, [isOpen]);

  function editProvider(provider: RegisteredProvider) {
    // The stored API key never crosses back to the frontend, so the field opens
    // empty with a note rather than pretending it is loaded.
    setSelectedKey(provider.key); setNewType(provider.key);
    setModels(provider.models.join(", ")); setApiKey(""); setError("");
  }

  async function saveProvider() {
    setError("");
    const target = isNew ? newType : selectedKey;
    if (!target) return setError("Choose a provider type to configure.");
    const modelList = models.split(",").map((model) => model.trim()).filter(Boolean);
    if (!modelList.length || !apiKey.trim()) return setError("Add at least one model and an API key.");
    setIsSaving(true);
    try {
      await invoke("save_provider_config", { provider: target, models: modelList, apiKey });
      const state = await invoke<SetupState>("load_setup_state");
      setProviders(state.providers);
      // Keep the saved provider open so its new models are visible right away.
      setSelectedKey(target); setNewType(target); setModels(modelList.join(", ")); setApiKey("");
    } catch (reason) {
      console.error("[SettingsPopup] save_provider_config rejected", { reason });
      setError(String(reason));
    } finally { setIsSaving(false); }
  }

  async function deleteProvider(key: string) {
    if (!window.confirm("Delete this provider from configuration?")) return;
    try {
      // try backend delete; if not available, fallback to reloading state
      await invoke("delete_provider", { provider: key });
      const state = await invoke<SetupState>("load_setup_state");
      setProviders(state.providers);
    } catch (reason) {
      console.warn("[SettingsPopup] delete_provider failed, falling back to local removal", { reason });
      setProviders((current) => current.filter((provider) => provider.key !== key));
    }
    startNewProvider();
  }

  if (!isOpen) return null;

  const activeSection = settingsSections.find(({ key }) => key === section) ?? settingsSections[0];

  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="project-dialog settings-dialog" role="dialog" aria-modal="true" aria-label="Settings">
        <nav className="settings-nav" aria-label="Settings sections">
          <p className="eyebrow">Settings</p>
          {settingsSections.map(({ key, label, detail }) => (
            <button
              className={key === section ? "settings-nav-item active" : "settings-nav-item"}
              key={key} type="button" aria-current={key === section ? "page" : undefined}
              onClick={() => setSection(key)}
            >
              <strong>{label}</strong><small>{detail}</small>
            </button>
          ))}
        </nav>
        <div className="settings-panel">
          <header className="settings-heading">
            <div><h2>{activeSection.label}</h2><p>{activeSection.detail}</p></div>
            <button className="icon-button" type="button" aria-label="Close settings" onClick={onClose}>&times;</button>
          </header>
          <div className="settings-body">
            {loading
              ? <p className="setup-note">Loading&hellip;</p>
              : <div className="providers-pane">
                  <div className="provider-sidebar" aria-label="Providers">
                    <button
                      className={isNew ? "provider-item active" : "provider-item"}
                      type="button" aria-pressed={isNew} onClick={startNewProvider}
                    >
                      <span className="provider-mark" aria-hidden="true">+</span>
                      <span className="provider-copy"><strong>New provider</strong><small>Add a provider</small></span>
                    </button>
                    {providers.map((provider) => (
                      <button
                        className={provider.key === selectedKey ? "provider-item active" : "provider-item"}
                        key={provider.key} type="button" aria-pressed={provider.key === selectedKey}
                        onClick={() => editProvider(provider)}
                      >
                        <span className="provider-mark" aria-hidden="true">{provider.name.slice(0, 1)}</span>
                        <span className="provider-copy">
                          <strong>{provider.name}</strong>
                          <small>{provider.api_key_set ? describeModels(provider.models) : "Not configured"}</small>
                        </span>
                      </button>
                    ))}
                    {!providers.length && <p className="setup-note">No providers are registered.</p>}
                  </div>
                  <div className="provider-editor">
                    <div className="provider-editor-heading">
                      <h3>{isNew ? "New provider" : selected?.name}</h3>
                      <p>{isNew ? "Pick a provider type, then add its credentials." : selected?.detail}</p>
                    </div>
                    {isNew && <label>Provider type
                      <select value={newType} onChange={(event) => { setNewType(event.target.value); setError(""); }}>
                        <option value="">Select type</option>
                        {providers.map(({ key, name }) => <option key={key} value={key}>{name}</option>)}
                      </select>
                    </label>}
                    <label>Models <span>Comma-separated</span>
                      <input value={models} autoComplete="off" spellCheck={false}
                        onChange={(event) => setModels(event.target.value)} placeholder="gemma4:31b-cloud, gemma4:9b" />
                    </label>
                    <label>API key {!isNew && selected?.api_key_set && <span>Saved &mdash; paste it again to replace</span>}
                      <input value={apiKey} type="password" autoComplete="off" spellCheck={false}
                        onChange={(event) => setApiKey(event.target.value)} placeholder="Paste your API key" />
                    </label>
                    {error && <p className="dialog-error" role="alert">{error}</p>}
                    <div className="provider-editor-actions">
                      {!isNew && selected?.api_key_set
                        ? <button className="secondary-button" type="button" onClick={() => void deleteProvider(selected.key)}>Delete provider</button>
                        : <span />}
                      <button className="add-button" type="button" onClick={() => void saveProvider()} disabled={isSaving}>
                        {isSaving ? "Saving…" : isNew ? "Add provider" : "Save changes"}
                      </button>
                    </div>
                  </div>
                </div>}
          </div>
        </div>
      </section>
    </div>
  );
}
