import { useState, useEffect } from "react";
import { check, Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

type UpdatePromptProps = {
  autoCheck?: boolean;
};

export default function UpdatePrompt({ autoCheck = true }: UpdatePromptProps) {
  const [update, setUpdate] = useState<Update | null>(null);
  const [isOpen, setIsOpen] = useState(false);
  const [isDownloading, setIsDownloading] = useState(false);
  const [downloadProgress, setDownloadProgress] = useState<number | null>(null);
  const [isDownloaded, setIsDownloaded] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!autoCheck) return;

    let mounted = true;
    async function checkForUpdate() {
      try {
        const updateResult = await check();
        if (mounted && updateResult && updateResult.available) {
          setUpdate(updateResult);
          setIsOpen(true);
        }
      } catch (err) {
        // Silently catch auto-check errors to avoid annoying the user on startup (e.g. offline)
        console.warn("[UpdatePrompt] Check failed:", err);
      }
    }

    void checkForUpdate();
    return () => {
      mounted = false;
    };
  }, [autoCheck]);

  async function handleDownloadAndInstall() {
    if (!update) return;

    setIsDownloading(true);
    setError(null);
    setDownloadProgress(0);

    try {
      let downloadedBytes = 0;
      let totalBytes = 0;

      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          totalBytes = event.data.contentLength ?? 0;
        } else if (event.event === "Progress") {
          downloadedBytes += event.data.chunkLength;
          if (totalBytes > 0) {
            setDownloadProgress(Math.min(100, Math.round((downloadedBytes / totalBytes) * 100)));
          }
        } else if (event.event === "Finished") {
          setDownloadProgress(100);
        }
      });

      setIsDownloaded(true);
    } catch (err) {
      console.error("[UpdatePrompt] Update installation failed:", err);
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsDownloading(false);
    }
  }

  async function handleRelaunch() {
    try {
      await relaunch();
    } catch (err) {
      console.error("[UpdatePrompt] Relaunch failed:", err);
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  if (!isOpen || !update) return null;

  return (
    <div className="dialog-backdrop" role="presentation" style={{ zIndex: 1100 }}>
      <div
        className="project-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="update-title"
        style={{ maxWidth: "480px" }}
      >
        <div className="dialog-heading">
          <div>
            <p className="eyebrow">Update Available</p>
            <h2 id="update-title">Version {update.version} is ready</h2>
          </div>
          {!isDownloading && (
            <button
              className="icon-button"
              type="button"
              aria-label="Close"
              onClick={() => setIsOpen(false)}
            >
              ×
            </button>
          )}
        </div>

        {update.body && (
          <div
            style={{
              maxHeight: "160px",
              overflowY: "auto",
              padding: "10px 12px",
              background: "#f9f8fe",
              border: "1px solid #ebe7fa",
              borderRadius: "8px",
              fontSize: "0.88rem",
              lineHeight: 1.4,
              color: "#4a4857",
              whiteSpace: "pre-wrap",
              marginBottom: "12px",
            }}
          >
            {update.body}
          </div>
        )}

        {isDownloading && (
          <div style={{ marginTop: "12px", marginBottom: "12px" }}>
            <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.85rem", marginBottom: "4px" }}>
              <span>Downloading update…</span>
              <span>{downloadProgress !== null ? `${downloadProgress}%` : ""}</span>
            </div>
            <div
              style={{
                width: "100%",
                height: "8px",
                background: "#eeeef3",
                borderRadius: "4px",
                overflow: "hidden",
              }}
            >
              <div
                style={{
                  width: `${downloadProgress ?? 0}%`,
                  height: "100%",
                  background: "#4e35ce",
                  transition: "width 0.2s ease-out",
                }}
              />
            </div>
          </div>
        )}

        {isDownloaded && (
          <p className="dialog-success" role="status" style={{ margin: "12px 0" }}>
            Update installed successfully! Restart Eggshell to apply.
          </p>
        )}

        {error && (
          <p className="dialog-error" role="alert" style={{ margin: "12px 0" }}>
            {error}
          </p>
        )}

        <div className="dialog-actions" style={{ marginTop: "16px" }}>
          {!isDownloaded ? (
            <>
              <button
                className="secondary-button"
                type="button"
                onClick={() => setIsOpen(false)}
                disabled={isDownloading}
              >
                Later
              </button>
              <button
                className="add-button"
                type="button"
                onClick={() => void handleDownloadAndInstall()}
                disabled={isDownloading}
              >
                {isDownloading ? "Updating…" : "Update now"}
              </button>
            </>
          ) : (
            <button
              className="add-button"
              type="button"
              onClick={() => void handleRelaunch()}
            >
              Restart Eggshell
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
