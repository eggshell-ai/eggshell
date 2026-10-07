import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";

import ProviderSettingsManager, {
  ProviderTypeDescriptor,
  RegisteredProvider,
} from "./ProviderSettingsManager";

type MysqlSummary = {
  kind: string;
  port: number;
  user: string;
  pass_set: boolean;
  is_mariadb?: boolean;
};

type SetupState = {
  setup_completed: boolean;
  providers: RegisteredProvider[];
  available_types?: ProviderTypeDescriptor[];
  mysql?: MysqlSummary;
  telemetry_enabled?: boolean;
  installation_id?: string;
};

type SettingsPopupProps = { isOpen: boolean; onClose: () => void };

// Add an entry here (and a matching panel in the body) to give the settings
// sidebar another section.
type SettingsSectionKey = "providers" | "mysql" | "privacy" | "about";
const settingsSections: { key: SettingsSectionKey; label: string; detail: string }[] = [
  { key: "providers", label: "Providers", detail: "Models and API keys" },
  { key: "mysql", label: "MySQL", detail: "Database connection & mode" },
  { key: "privacy", label: "Data & Privacy", detail: "Telemetry and crash reporting" },
  { key: "about", label: "About", detail: "Version and links" },
];

export default function SettingsPopup({ isOpen, onClose }: SettingsPopupProps) {
  const [providers, setProviders] = useState<RegisteredProvider[]>([]);
  const [availableTypes, setAvailableTypes] = useState<ProviderTypeDescriptor[]>([]);
  const [loading, setLoading] = useState(false);
  const [section, setSection] = useState<SettingsSectionKey>("providers");

  // MySQL settings state
  const [mysqlKind, setMysqlKind] = useState<"managed" | "external">("managed");
  const [mysqlPort, setMysqlPort] = useState<number>(3306);
  const [mysqlUser, setMysqlUser] = useState<string>("root");
  const [mysqlPass, setMysqlPass] = useState<string>("");
  const [mysqlPassSet, setMysqlPassSet] = useState<boolean>(false);
  const [mysqlIsMariadb, setMysqlIsMariadb] = useState<boolean>(false);
  const [mysqlError, setMysqlError] = useState<string>("");
  const [mysqlSuccess, setMysqlSuccess] = useState<string>("");
  const [isSavingMysql, setIsSavingMysql] = useState<boolean>(false);

  // Privacy & telemetry state
  const [telemetryEnabled, setTelemetryEnabled] = useState<boolean>(false);
  const [installationId, setInstallationId] = useState<string>("");
  const [privacyError, setPrivacyError] = useState<string>("");
  const [privacySuccess, setPrivacySuccess] = useState<string>("");
  const [isSavingPrivacy, setIsSavingPrivacy] = useState<boolean>(false);

  // About state
  const [appVersion, setAppVersion] = useState<string>("");

  useEffect(() => {
    if (!isOpen) return;
    setSection("providers");
    setMysqlPass("");
    setMysqlError("");
    setMysqlSuccess("");
    setPrivacyError("");
    setPrivacySuccess("");
    setLoading(true);
    void invoke<SetupState>("load_setup_state")
      .then(({ providers: loaded, available_types: types, mysql, telemetry_enabled, installation_id }) => {
        setProviders(loaded);
        if (types && types.length > 0) {
          setAvailableTypes(types);
        } else {
          setAvailableTypes([
            { key: "ollama", name: "Ollama", detail: "Cloud and local models from ollama.com" },
            { key: "openai", name: "OpenAI compatible", detail: "Any OpenAI-compatible chat completions API" },
          ]);
        }
        if (mysql) {
          setMysqlKind(mysql.kind === "external" ? "external" : "managed");
          setMysqlPort(mysql.port || 3306);
          setMysqlUser(mysql.user || "root");
          setMysqlPassSet(Boolean(mysql.pass_set));
          setMysqlIsMariadb(Boolean(mysql.is_mariadb));
        }
        if (telemetry_enabled !== undefined) {
          setTelemetryEnabled(Boolean(telemetry_enabled));
        }
        if (installation_id) {
          setInstallationId(installation_id);
        }
      })
      .catch((reason: unknown) => console.error("[SettingsPopup] load_setup_state rejected", { reason }))
      .finally(() => setLoading(false));
    void getVersion().then(setAppVersion).catch(() => setAppVersion("unknown"));
  }, [isOpen]);

  async function saveMysqlSettings() {
    setMysqlError("");
    setMysqlSuccess("");
    const parsedPort = Number(mysqlPort);
    if (!parsedPort || isNaN(parsedPort) || parsedPort < 1 || parsedPort > 65535) {
      setMysqlError("Please specify a valid port number between 1 and 65535.");
      return;
    }
    if (!mysqlUser.trim()) {
      setMysqlError("MySQL user cannot be empty.");
      return;
    }

    setIsSavingMysql(true);
    try {
      await invoke("save_mysql_settings", {
        kind: mysqlKind,
        port: parsedPort,
        user: mysqlUser.trim(),
        pass: mysqlPass ? mysqlPass : null,
        is_mariadb: mysqlIsMariadb,
      });

      if (mysqlPass.trim()) {
        setMysqlPassSet(true);
        setMysqlPass("");
      }

      setMysqlSuccess("MySQL settings saved successfully.");
    } catch (reason) {
      console.error("[SettingsPopup] save_mysql_settings rejected", { reason });
      setMysqlError(String(reason));
    } finally {
      setIsSavingMysql(false);
    }
  }

  async function savePrivacySettings() {
    setIsSavingPrivacy(true);
    setPrivacyError("");
    setPrivacySuccess("");
    try {
      await invoke("save_privacy_settings", { enabled: telemetryEnabled });
      setPrivacySuccess("Privacy settings saved successfully.");
    } catch (reason) {
      console.error("[SettingsPopup] save_privacy_settings rejected", { reason });
      setPrivacyError(String(reason));
    } finally {
      setIsSavingPrivacy(false);
    }
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
            {loading ? (
              <p className="setup-note">Loading&hellip;</p>
            ) : section === "mysql" ? (
              <div className="settings-form-pane">
                <label>
                  Type
                  <select
                    value={mysqlKind}
                    onChange={(e) => {
                      setMysqlKind(e.target.value as "managed" | "external");
                      setMysqlError("");
                      setMysqlSuccess("");
                    }}
                  >
                    <option value="managed">Managed (Eggshell automatically starts and manages MySQL)</option>
                    <option value="external">External (Connect to an existing local or remote MySQL service)</option>
                  </select>
                </label>
                <span className="settings-field-hint">
                  {mysqlKind === "managed"
                    ? "Eggshell will automatically start the portable MySQL instance when needed."
                    : "Use an existing MySQL instance running on this machine or a network server."}
                </span>

                <label>
                  Port <span>Standard MySQL port is 3306</span>
                  <input
                    type="number"
                    min={1}
                    max={65535}
                    value={mysqlPort}
                    onChange={(e) => {
                      setMysqlPort(parseInt(e.target.value, 10) || 0);
                      setMysqlError("");
                      setMysqlSuccess("");
                    }}
                    placeholder="3306"
                  />
                </label>

                <label>
                  User <span>Default administrative user is root</span>
                  <input
                    type="text"
                    autoComplete="off"
                    spellCheck={false}
                    value={mysqlUser}
                    onChange={(e) => {
                      setMysqlUser(e.target.value);
                      setMysqlError("");
                      setMysqlSuccess("");
                    }}
                    placeholder="root"
                  />
                </label>

                <label>
                  Password {mysqlPassSet && <span>Saved &mdash; leave empty to keep unchanged</span>}
                  <input
                    type="password"
                    autoComplete="off"
                    spellCheck={false}
                    value={mysqlPass}
                    onChange={(e) => {
                      setMysqlPass(e.target.value);
                      setMysqlError("");
                      setMysqlSuccess("");
                    }}
                    placeholder={mysqlPassSet ? "Enter new password to change" : "Leave empty if root has no password"}
                  />
                </label>

                <label className="settings-checkbox-label">
                  <input
                    type="checkbox"
                    checked={mysqlIsMariadb}
                    onChange={(e) => {
                      setMysqlIsMariadb(e.target.checked);
                      setMysqlError("");
                      setMysqlSuccess("");
                    }}
                  />
                  <span>Use MariaDB</span>
                </label>
                <span className="settings-field-hint">
                  Enable this if your database server is MariaDB rather than standard MySQL. This configures Symfony and Doctrine with MariaDB compatibility.
                </span>

                {mysqlSuccess && <p className="dialog-success" role="status">{mysqlSuccess}</p>}
                {mysqlError && <p className="dialog-error" role="alert">{mysqlError}</p>}

                <div className="provider-editor-actions">
                  <span />
                  <div className="provider-editor-buttons">
                    <button
                      className="add-button"
                      type="button"
                      onClick={() => void saveMysqlSettings()}
                      disabled={isSavingMysql}
                    >
                      {isSavingMysql ? "Saving…" : "Save MySQL settings"}
                    </button>
                  </div>
                </div>
              </div>
            ) : section === "privacy" ? (
              <div className="settings-form-pane">
                <label className="settings-checkbox-label">
                  <input
                    type="checkbox"
                    checked={telemetryEnabled}
                    onChange={(e) => {
                      setTelemetryEnabled(e.target.checked);
                      setPrivacyError("");
                      setPrivacySuccess("");
                    }}
                  />
                  <span>Enable telemetry & crash reporting</span>
                </label>
                <span className="settings-field-hint" style={{ fontWeight: 600, color: "#6246ea" }}>
                  We never capture your prompts or sensitive data
                </span>
                <span className="settings-field-hint">
                  When enabled, anonymous crash reports, runtime errors, application start, and conversation start events are shared with Sentry to help us diagnose issues and improve application stability.
                </span>

                {installationId ? (
                  <label style={{ marginTop: "16px" }}>
                    Installation ID <span>Anonymous identifier associated with telemetry events</span>
                    <input
                      type="text"
                      readOnly
                      value={installationId}
                      style={{ background: "#fbfaff", cursor: "default" }}
                    />
                  </label>
                ) : null}

                {privacySuccess && <p className="dialog-success" role="status">{privacySuccess}</p>}
                {privacyError && <p className="dialog-error" role="alert">{privacyError}</p>}

                <div className="provider-editor-actions">
                  <span />
                  <div className="provider-editor-buttons">
                    <button
                      className="add-button"
                      type="button"
                      onClick={() => void savePrivacySettings()}
                      disabled={isSavingPrivacy}
                    >
                      {isSavingPrivacy ? "Saving…" : "Save privacy settings"}
                    </button>
                  </div>
                </div>
              </div>
            ) : section === "about" ? (
              <div className="settings-form-pane">
                <label>
                  Version
                  <input
                    type="text"
                    readOnly
                    value={appVersion || "Loading…"}
                    style={{ background: "#fbfaff", cursor: "default" }}
                  />
                </label>

                <div style={{ marginTop: "8px" }}>
                  <strong style={{ display: "block", marginBottom: "4px" }}>Links</strong>
                  <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
                    <button
                      className="secondary-button"
                      type="button"
                      style={{ textAlign: "left" }}
                      onClick={() => void openUrl("https://discord.gg/y68jFEVDVN")}
                    >
                      Discord &mdash; Join our community
                    </button>
                    <button
                      className="secondary-button"
                      type="button"
                      style={{ textAlign: "left" }}
                      onClick={() => void openUrl("https://github.com/eggshell-ai/eggshell")}
                    >
                      GitHub &mdash; Source code &amp; issues
                    </button>
                  </div>
                </div>
              </div>
            ) : (
              <ProviderSettingsManager
                providers={providers}
                setProviders={setProviders}
                availableTypes={availableTypes}
                setAvailableTypes={setAvailableTypes}
              />
            )}
          </div>
        </div>
      </section>
    </div>
  );
}
