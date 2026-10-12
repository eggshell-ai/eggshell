import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export type ProviderTypeDescriptor = { key: string; name: string; detail: string };

export type RegisteredProvider = {
  id: string;
  key: string;
  provider_type: string;
  name: string;
  title: string;
  detail: string;
  base_url?: string | null;
  api_key_set: boolean;
  models: string[];
};

type SetupState = {
  setup_completed: boolean;
  providers: RegisteredProvider[];
  available_types?: ProviderTypeDescriptor[];
};

export type ProviderSettingsManagerProps = {
  providers: RegisteredProvider[];
  setProviders: React.Dispatch<React.SetStateAction<RegisteredProvider[]>>;
  availableTypes: ProviderTypeDescriptor[];
  setAvailableTypes?: React.Dispatch<React.SetStateAction<ProviderTypeDescriptor[]>>;
  mode?: "settings" | "setup";
  submitButtonLabel?: string;
  onSaved?: (savedProvider: RegisteredProvider) => void;
};

function describeModels(models: string[]) {
  if (!models.length) return "No models yet";
  return models.length === 1 ? models[0] : `${models.length} models`;
}

export default function ProviderSettingsManager({
  providers,
  setProviders,
  availableTypes,
  setAvailableTypes,
  mode = "settings",
  submitButtonLabel,
  onSaved,
}: ProviderSettingsManagerProps) {
  const [selectedId, setSelectedId] = useState<string | null>(() => {
    if (mode === "setup" && providers.length > 0) {
      return providers[0].id || providers[0].key;
    }
    return null;
  });

  const [selectedType, setSelectedType] = useState(() => {
    if (mode === "setup" && providers.length > 0) {
      return providers[0].provider_type || providers[0].name.toLowerCase() || "";
    }
    return "";
  });

  const [title, setTitle] = useState(() => {
    if (mode === "setup" && providers.length > 0) {
      return providers[0].title || providers[0].name || "";
    }
    return "";
  });

  const [baseUrl, setBaseUrl] = useState(() => {
    if (mode === "setup" && providers.length > 0) {
      return providers[0].base_url || "";
    }
    return "";
  });

  const [models, setModels] = useState<string[]>(() => {
    if (mode === "setup" && providers.length > 0) {
      return [...providers[0].models];
    }
    return [];
  });

  const [draftModel, setDraftModel] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [error, setError] = useState("");
  const [testStatus, setTestStatus] = useState<{ type: "success" | "error"; message: string } | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  const [isTesting, setIsTesting] = useState(false);
  const [testingSidebarId, setTestingSidebarId] = useState<string | null>(null);

  const selected = providers.find((p) => p.id === selectedId || p.key === selectedId) ?? null;
  const isNew = selectedId === null;
  const currentType = isNew ? selectedType : (selected?.provider_type || selected?.name?.toLowerCase() || "");

  function startNewProvider() {
    setSelectedId(null);
    setSelectedType("");
    setTitle("");
    setBaseUrl("");
    setModels([]);
    setDraftModel("");
    setApiKey("");
    setError("");
    setTestStatus(null);
  }

  function addDraftModel() {
    const value = draftModel.trim();
    if (!value) return;
    setModels((current) => (current.includes(value) ? current : [...current, value]));
    setDraftModel("");
    setError("");
  }

  function renameModel(index: number, value: string) {
    setModels((current) => current.map((model, position) => (position === index ? value : model)));
  }

  function commitModel(index: number) {
    setModels((current) => {
      const value = current[index].trim();
      if (!value) return current.filter((_, position) => position !== index);
      return current.map((model, position) => (position === index ? value : model));
    });
  }

  function removeModel(index: number) {
    setModels((current) => current.filter((_, position) => position !== index));
  }

  function editProvider(provider: RegisteredProvider) {
    setSelectedId(provider.id || provider.key);
    setSelectedType(provider.provider_type || provider.name.toLowerCase());
    setTitle(provider.title || provider.name);
    setBaseUrl(provider.base_url || "");
    setModels([...provider.models]);
    setDraftModel("");
    setApiKey("");
    setError("");
    setTestStatus(null);
  }

  async function testProvider() {
    setError("");
    setTestStatus(null);
    const typeToUse = isNew ? selectedType : (selected?.provider_type || selectedType);
    if (!typeToUse) {
      setError("Choose a provider type to configure.");
      return;
    }

    const keepsSavedKey = !isNew && Boolean(selected?.api_key_set);
    if (!apiKey.trim() && !keepsSavedKey && typeToUse !== "ollama") {
      setError("An API key is required to test the connection.");
      return;
    }

    setIsTesting(true);
    try {
      const fetched = await invoke<string[]>("test_provider_config", {
        provider: selectedId || typeToUse,
        providerId: selectedId ?? null,
        providerType: typeToUse,
        baseUrl: baseUrl.trim() || null,
        apiKey,
      });

      if (fetched && fetched.length > 0) {
        setModels((current) => {
          if (current.length > 0) return current;
          return [...fetched];
        });
      }

      setTestStatus({
        type: "success",
        message: `Connection successful! Fetched ${fetched.length} model${fetched.length === 1 ? "" : "s"}.`,
      });
    } catch (reason) {
      console.error("[ProviderSettingsManager] test_provider_config rejected", { reason });
      setError(String(reason));
      setTestStatus({
        type: "error",
        message: String(reason),
      });
    } finally {
      setIsTesting(false);
    }
  }

  async function testSavedProvider(provider: RegisteredProvider) {
    const id = provider.id || provider.key;
    setTestingSidebarId(id);
    setError("");
    setTestStatus(null);
    try {
      const fetched = await invoke<string[]>("test_provider_config", {
        provider: id,
        providerId: id,
        providerType: provider.provider_type || provider.name.toLowerCase(),
        baseUrl: provider.base_url || null,
        apiKey: "",
      });

      if (fetched && fetched.length > 0) {
        setProviders((current) =>
          current.map((p) => {
            if ((p.id || p.key) === id) {
              if (p.models && p.models.length > 0) return p;
              return { ...p, models: [...fetched] };
            }
            return p;
          })
        );
        if (selectedId === id) {
          setModels((current) => {
            if (current.length > 0) return current;
            return [...fetched];
          });
        }
      }

      setTestStatus({
        type: "success",
        message: `Connection to "${provider.title || provider.name}" verified! Fetched ${fetched.length} model${fetched.length === 1 ? "" : "s"}.`,
      });
    } catch (reason) {
      console.error("[ProviderSettingsManager] testSavedProvider rejected", { reason });
      setError(`Failed to connect to ${provider.title || provider.name}: ${String(reason)}`);
    } finally {
      setTestingSidebarId(null);
    }
  }

  async function saveProvider() {
    setError("");
    setTestStatus(null);
    const typeToUse = isNew ? selectedType : (selected?.provider_type || selectedType);
    if (!typeToUse) return setError("Choose a provider type to configure.");

    const combinedModels = draftModel.trim() ? [...models, draftModel.trim()] : models;
    const modelList = Array.from(new Set(combinedModels.map((model) => model.trim()).filter(Boolean)));
    const keepsSavedKey = !isNew && Boolean(selected?.api_key_set);
    if (!apiKey.trim() && !keepsSavedKey && typeToUse !== "ollama") return setError("An API key is required.");

    setIsSaving(true);
    try {
      await invoke("save_provider_config", {
        provider: selectedId || typeToUse,
        providerId: selectedId ?? null,
        providerType: typeToUse,
        title: title.trim() || null,
        baseUrl: baseUrl.trim() || null,
        models: modelList,
        apiKey,
      });
      const state = await invoke<SetupState>("load_setup_state");
      setProviders(state.providers);
      if (state.available_types && setAvailableTypes) {
        setAvailableTypes(state.available_types);
      }

      const updated = state.providers.find((p) =>
        selectedId ? (p.id === selectedId || p.key === selectedId) : (p.title === title || p.provider_type === typeToUse)
      ) ?? state.providers[state.providers.length - 1];

      if (updated) {
        editProvider(updated);
      } else {
        startNewProvider();
      }

      setTestStatus({
        type: "success",
        message: "Connection verified and provider saved successfully!",
      });

      if (onSaved && updated) {
        onSaved(updated);
      }
    } catch (reason) {
      console.error("[ProviderSettingsManager] save_provider_config rejected", { reason });
      setError(String(reason));
    } finally {
      setIsSaving(false);
    }
  }

  async function deleteProvider(id: string) {
    if (!window.confirm("Delete this provider from configuration?")) return;
    try {
      await invoke("delete_provider", { provider: id });
      const state = await invoke<SetupState>("load_setup_state");
      setProviders(state.providers);
    } catch (reason) {
      console.warn("[ProviderSettingsManager] delete_provider failed, falling back to local removal", { reason });
      setProviders((current) => current.filter((provider) => provider.id !== id && provider.key !== id));
    }
    startNewProvider();
  }

  const defaultSaveLabel = isSaving ? "Testing & saving…" : isNew ? "Add provider" : "Save changes";
  const saveLabel = submitButtonLabel ? (isSaving ? "Testing & saving…" : submitButtonLabel) : defaultSaveLabel;

  return (
    <div className="providers-pane">
      <div className="provider-sidebar" aria-label="Providers">
        <button
          className={isNew ? "provider-item active" : "provider-item"}
          type="button"
          aria-pressed={isNew}
          onClick={startNewProvider}
        >
          <span className="provider-mark" aria-hidden="true">+</span>
          <span className="provider-copy"><strong>New provider</strong><small>Add a provider</small></span>
        </button>
        {providers.map((p) => {
          const id = p.id || p.key;
          const isItemActive = id === selectedId;
          const displayTitle = p.title || p.name;
          const isTestingThis = testingSidebarId === id;
          return (
            <div
              className={isItemActive ? "provider-item active" : "provider-item"}
              key={id}
              role="button"
              tabIndex={0}
              aria-pressed={isItemActive}
              onClick={() => editProvider(p)}
              onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") {
                  event.preventDefault();
                  editProvider(p);
                }
              }}
            >
              <span className="provider-mark" aria-hidden="true">{displayTitle.slice(0, 1).toUpperCase()}</span>
              <span className="provider-copy">
                <strong>{displayTitle}</strong>
                <small>{p.api_key_set ? describeModels(p.models) : "Not configured"}</small>
              </span>
              <button
                className="provider-item-test-btn"
                type="button"
                title={`Test connection for ${displayTitle}`}
                aria-label={`Test connection for ${displayTitle}`}
                disabled={isTestingThis || isTesting || isSaving}
                onClick={(event) => {
                  event.stopPropagation();
                  void testSavedProvider(p);
                }}
              >
                {isTestingThis ? "…" : "Test"}
              </button>
            </div>
          );
        })}
        {!providers.length && <p className="setup-note">No providers are configured yet.</p>}
      </div>

      <div className="provider-editor">
        <div className="provider-editor-heading">
          <h3>{isNew ? "New provider" : (selected?.title || selected?.name)}</h3>
          <p>{isNew ? "Pick a provider type, then add its credentials." : selected?.detail}</p>
        </div>

        {isNew ? (
          <label>Provider type
            <select
              value={selectedType}
              onChange={(event) => {
                const newType = event.target.value;
                setSelectedType(newType);
                const matched = availableTypes.find((t) => t.key === newType);
                if (matched && !title) setTitle(matched.name);
                setError("");
              }}
            >
              <option value="">Select type</option>
              {availableTypes.map(({ key, name }) => <option key={key} value={key}>{name}</option>)}
            </select>
          </label>
        ) : null}

        <label>Title <span>Custom display name</span>
          <input
            value={title}
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => setTitle(event.target.value)}
            placeholder="e.g. Local Ollama, OpenRouter, Work OpenAI"
          />
        </label>

        {currentType === "openai" && (
          <label>Base URL <span>Optional &mdash; defaults to https://api.openai.com/v1</span>
            <input
              value={baseUrl}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => setBaseUrl(event.target.value)}
              placeholder="https://api.openai.com/v1 or http://localhost:1234/v1"
            />
          </label>
        )}

        <div className="model-field">
          <div className="model-field-label">Models <span>Optional &mdash; fetched automatically when left empty</span></div>
          {models.length > 0 && (
            <ul className="model-list">
              {models.map((model, index) => (
                <li className="model-row" key={index}>
                  <input
                    value={model}
                    autoComplete="off"
                    spellCheck={false}
                    aria-label={`Model ${index + 1}`}
                    onChange={(event) => renameModel(index, event.target.value)}
                    onBlur={() => commitModel(index)}
                  />
                  <button
                    className="model-remove"
                    type="button"
                    aria-label={`Remove ${model || "model"}`}
                    onMouseDown={(event) => event.preventDefault()}
                    onClick={() => removeModel(index)}
                  >
                    &times;
                  </button>
                </li>
              ))}
            </ul>
          )}
          {!models.length && (
            <p className="model-empty">
              No models yet &mdash; we&apos;ll fetch this provider&apos;s models automatically once it is connected.
              You can add them by hand below in the meantime.
            </p>
          )}
          <div className="model-add">
            <input
              value={draftModel}
              autoComplete="off"
              spellCheck={false}
              aria-label="Add a model"
              placeholder="Add a model, e.g. gemma4:31b-cloud"
              onChange={(event) => setDraftModel(event.target.value)}
              onKeyDown={(event) => {
                if (event.key !== "Enter") return;
                event.preventDefault();
                addDraftModel();
              }}
            />
            <button
              className="secondary-button"
              type="button"
              onClick={addDraftModel}
              disabled={!draftModel.trim()}
            >
              Add
            </button>
          </div>
        </div>

        <label>API key {!isNew && selected?.api_key_set && <span>Saved &mdash; paste it again to replace</span>}
          <input
            value={apiKey}
            type="password"
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => setApiKey(event.target.value)}
            placeholder="Paste your API key"
          />
        </label>

        {testStatus && testStatus.type === "success" && (
          <p className="dialog-success" role="status">{testStatus.message}</p>
        )}
        {error && <p className="dialog-error" role="alert">{error}</p>}

        <div className="provider-editor-actions">
          {!isNew && selected && mode === "settings" ? (
            <button
              className="secondary-button"
              type="button"
              onClick={() => void deleteProvider(selected.id || selected.key)}
            >
              Delete provider
            </button>
          ) : (
            <span />
          )}
          <div className="provider-editor-buttons">
            <button
              className="secondary-button"
              type="button"
              onClick={() => void testProvider()}
              disabled={isTesting || isSaving}
            >
              {isTesting ? "Testing…" : "Test"}
            </button>
            <button
              className="add-button"
              type="button"
              onClick={() => void saveProvider()}
              disabled={isSaving || isTesting}
            >
              {saveLabel}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

