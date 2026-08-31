import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";

const bugReportUrl = "https://github.com/eggshell-ai/eggshell/issues/new?template=bug_report.yml";
const featureRequestUrl = "https://github.com/eggshell-ai/eggshell/issues/new?template=feature_request.yml";

export default function ReportMenu() {
  const [isOpen, setIsOpen] = useState(false);
  async function openReport(url: string) { setIsOpen(false); await openUrl(url); }
  return <div className="report-menu">
    {isOpen && <div className="report-options" role="menu" aria-label="Report an issue">
      <button type="button" role="menuitem" onClick={() => void openReport(bugReportUrl)}>Report a Bug</button>
      <button type="button" role="menuitem" onClick={() => void openReport(featureRequestUrl)}>Request a Feature</button>
    </div>}
    <button className="report-button" type="button" aria-label="Report an issue" aria-expanded={isOpen} aria-haspopup="menu" onClick={() => setIsOpen((current) => !current)}><span aria-hidden="true">✦</span></button>
  </div>;
}
