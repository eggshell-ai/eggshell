import { useState } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";

export type IterationLoopData = {
  isIterationLoop?: boolean;
  loopNumber?: number;
  plan?: string;
  implementation?: string;
  qaReport?: string;
  status?: "running" | "completed" | "wrapped_up" | "force_stopped";
};

type IterationLoopCardProps = {
  data: IterationLoopData;
  isCurrent?: boolean;
};

export default function IterationLoopCard({ data, isCurrent }: IterationLoopCardProps) {
  const [activeTab, setActiveTab] = useState<"plan" | "implementation" | "qa">("qa");
  const [isCollapsed, setIsCollapsed] = useState(!isCurrent);

  const loopNum = data.loopNumber ?? 1;
  const isWrappedUp = data.status === "wrapped_up";
  const isForceStopped = data.status === "force_stopped";

  return (
    <article className={`iteration-loop-card ${isCurrent ? "current" : "finished"}`}>
      <header className="iteration-card-header" onClick={() => setIsCollapsed(!isCollapsed)}>
        <div className="iteration-header-left">
          <span className="iteration-badge">Loop #{loopNum}</span>
          <span className="iteration-title">Autonomous Iteration Cycle</span>
          {isCurrent ? (
            <span className="iteration-status-pill running">
              <span className="iteration-spinner" /> Active
            </span>
          ) : isWrappedUp ? (
            <span className="iteration-status-pill wrapped-up">✓ Wrapped Up</span>
          ) : isForceStopped ? (
            <span className="iteration-status-pill force-stopped">Stopped</span>
          ) : (
            <span className="iteration-status-pill completed">✓ Completed</span>
          )}
        </div>
        <button
          type="button"
          className="iteration-collapse-btn"
          aria-label={isCollapsed ? "Expand iteration details" : "Collapse iteration details"}
        >
          {isCollapsed ? "Expand ⌄" : "Collapse ⌃"}
        </button>
      </header>

      {!isCollapsed && (
        <div className="iteration-card-body">
          <nav className="iteration-tabs" aria-label="Iteration stage tabs">
            <button
              type="button"
              className={`iteration-tab ${activeTab === "plan" ? "active" : ""}`}
              onClick={() => setActiveTab("plan")}
            >
              📋 Plan
            </button>
            <button
              type="button"
              className={`iteration-tab ${activeTab === "implementation" ? "active" : ""}`}
              onClick={() => setActiveTab("implementation")}
            >
              ⚙️ Changes
            </button>
            <button
              type="button"
              className={`iteration-tab ${activeTab === "qa" ? "active" : ""}`}
              onClick={() => setActiveTab("qa")}
            >
              🧪 QA Audit
            </button>
          </nav>

          <div className="iteration-tab-content">
            {activeTab === "plan" && (
              <div className="tab-pane plan-pane">
                {data.plan ? (
                  <Markdown remarkPlugins={[remarkGfm]}>{data.plan}</Markdown>
                ) : (
                  <p className="empty-tab-text">Planning in progress…</p>
                )}
              </div>
            )}
            {activeTab === "implementation" && (
              <div className="tab-pane impl-pane">
                {data.implementation ? (
                  <Markdown remarkPlugins={[remarkGfm]}>{data.implementation}</Markdown>
                ) : (
                  <p className="empty-tab-text">Implementation in progress…</p>
                )}
              </div>
            )}
            {activeTab === "qa" && (
              <div className="tab-pane qa-pane">
                {data.qaReport ? (
                  <Markdown remarkPlugins={[remarkGfm]}>{data.qaReport}</Markdown>
                ) : (
                  <p className="empty-tab-text">QA testing in progress…</p>
                )}
              </div>
            )}
          </div>
        </div>
      )}
    </article>
  );
}
