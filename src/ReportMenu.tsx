import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";

const bugReportUrl = "https://github.com/eggshell-ai/eggshell/issues/new?template=bug_report.yml";
const featureRequestUrl = "https://github.com/eggshell-ai/eggshell/issues/new?template=feature_request.yml";

type ReportType = "bug" | "feature";
// Mirrors `logger::LogEntry`: the central log the backend accumulates for reporting.
type LogEntry = { level: "info" | "warning" | "error"; message: string; sensitive: boolean };
type ReportMenuProps = {
  screenName: "Setup" | "provider" | "Home" | "Project";
  /** Accumulated central-log entries, included in the copied diagnostics. */
  accumulatedLogs?: LogEntry[];
};

export default function ReportMenu({ screenName, accumulatedLogs = [] }: ReportMenuProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [reportType, setReportType] = useState<ReportType | null>(null);
  const [includePromptData, setIncludePromptData] = useState(true);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState("");

  function chooseReport(type: ReportType) {
    setIsOpen(false); setReportType(type); setIncludePromptData(true); setCopied(false); setError("");
  }

  async function openReport() {
    const url = reportType === "bug" ? bugReportUrl : featureRequestUrl;
    try {
      // Sensitive entries (the prompts) are only fetched when the user opted in;
      // otherwise the report quotes the public log the frontend already holds.
      let logs = accumulatedLogs;
      if (includePromptData) {
        logs = await invoke<LogEntry[]>("read_logs", { includeSensitive: true });
      }
      const logSection = logs.length > 0
        ? "\n\nLogs:\n" + logs.map(({ level, message }) => `[${level}] ${message}`).join("\n")
        : "";
      await navigator.clipboard.writeText(`Screen Name: ${screenName}${logSection}`);
      await openUrl(url);
      setCopied(true);
    } catch (reason) { setError(`Diagnostics could not be copied. ${String(reason)}`); }
  }

  return <div className="report-menu">
    {isOpen && <div className="report-options" role="menu" aria-label="Report an issue">
      <button type="button" role="menuitem" onClick={() => chooseReport("bug")}>Report a Bug</button>
      <button type="button" role="menuitem" onClick={() => chooseReport("feature")}>Request a Feature</button>
    </div>}
    <button className="report-button" type="button" aria-label="Report an issue" aria-expanded={isOpen} aria-haspopup="menu" onClick={() => setIsOpen((current) => !current)}><span aria-hidden="true">✦</span></button>
    {reportType && <div className="dialog-backdrop" role="presentation"><section className="report-dialog" role="dialog" aria-modal="true" aria-labelledby="report-dialog-title">
      <div className="dialog-heading"><div><p className="eyebrow">Diagnostics</p><h2 id="report-dialog-title">{copied ? "Diagnostics copied" : "Include diagnostics?"}</h2></div>{!copied && <button className="icon-button" type="button" aria-label="Close" onClick={() => setReportType(null)}>×</button>}</div>
      {copied
        ? <><p className="dialog-note">Paste the copied diagnostics in <strong>{reportType === "bug" ? "Logs for Bug Report" : "Additional Context"}</strong> in the issue form that just opened.</p><div className="dialog-actions"><button className="add-button" type="button" onClick={() => setReportType(null)}>Done</button></div></>
        : <><label className="diagnostics-check"><input type="checkbox" checked={includePromptData} onChange={(event) => setIncludePromptData(event.target.checked)} /> Include prompt data</label><p className="dialog-note">The copied diagnostics will include the screen name{accumulatedLogs.length > 0 ? ` and ${accumulatedLogs.length} accumulated log entries` : ""}{includePromptData ? ", plus prompt data from agent conversations" : ""}.</p>{error && <p className="dialog-error" role="alert">{error}</p>}<div className="dialog-actions"><button className="secondary-button" type="button" onClick={() => setReportType(null)}>Cancel</button><button className="add-button" type="button" onClick={() => void openReport()}>Open</button></div></>}
    </section></div>}
  </div>;
}
