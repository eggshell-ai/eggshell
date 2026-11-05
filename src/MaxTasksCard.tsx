export type MaxTask = {
  id: number;
  project_id: number;
  session_id: number;
  task_index: number;
  title: string;
  description: string;
  /** One of: pending, running, completed, failed, skipped. */
  status: string;
  result_summary?: string | null;
};

export type MaxPlanData = {
  isMaxPlan?: boolean;
  maxTaskCount?: number;
  maxTasks?: MaxTask[];
};

type MaxTasksCardProps = {
  data: MaxPlanData;
  /** The task list currently stored for the session (overrides the snapshot). */
  tasks?: MaxTask[];
  /** The planner's full Markdown plan, shown behind a disclosure toggle. */
  plan?: string;
  isSending?: boolean;
  onRun: () => void;
  onRetryFailed: () => void;
  onStopTask?: (taskId: number) => void;
};

/**
 * Splits a task description into its one-line summary and the remaining
 * detail. The planner emits a short intro followed by `### Entity & Fields`,
 * `### Screen & Menu` and `### Build Steps` bullets; the list stays compact by
 * showing only the intro and hiding the rest behind a toggle.
 */
export function splitTaskDescription(description: string): { summary: string; details: string } {
  const lines = description.split(/\r?\n/);
  const summaryLines: string[] = [];
  let index = 0;
  for (; index < lines.length; index += 1) {
    const line = lines[index].trim();
    if (!line || line.startsWith("#") || line.startsWith("-") || line.startsWith("*")) break;
    summaryLines.push(line);
  }
  return {
    summary: summaryLines.join(" ").trim(),
    details: lines.slice(index).join("\n").trim(),
  };
}

function statusLabel(status: string): string {
  switch (status) {
    case "completed":
      return "✓ Done";
    case "running":
      return "Running…";
    case "failed":
      return "✕ Failed";
    case "paused":
      return "⏸ Turn limit reached";
    case "skipped":
      return "Skipped";
    default:
      return "Pending";
  }
}

/**
 * Renders the Max mode plan: the sectioned breakdown produced in Phase 1 —
 * each task with its entity/fields/screen detail behind a disclosure — plus
 * per-task status and a control to start/resume Phase 2.
 */
export default function MaxTasksCard({ data, tasks, plan, isSending = false, onRun, onRetryFailed, onStopTask }: MaxTasksCardProps) {
  const list = tasks && tasks.length > 0 ? tasks : data.maxTasks ?? [];
  const total = list.length || data.maxTaskCount || 0;
  const completed = list.filter((task) => task.status === "completed").length;
  const pausedTask = list.find((task) => task.status === "paused");
  const hasPaused = Boolean(pausedTask);
  const hasFailed = list.some((task) => task.status === "failed");
  const isRunning = list.some((task) => task.status === "running");
  const hasPending = list.some((task) => task.status === "pending" || task.status === "failed" || task.status === "paused");
  const allDone = total > 0 && completed === total;

  return (
    <article className={`max-tasks-card ${isRunning ? "running" : allDone ? "done" : ""}`}>
      <header className="max-card-header">
        <div className="max-header-left">
          <span className="max-badge">🚀 Max</span>
          <span className="max-title">Build Plan</span>
          {isRunning ? (
            <span className="max-status-pill running">
              <span className="max-spinner" /> Running
            </span>
          ) : allDone ? (
            <span className="max-status-pill completed">✓ All tasks complete</span>
          ) : hasPaused ? (
            <span className="max-status-pill paused">Paused — turn limit reached</span>
          ) : hasFailed ? (
            <span className="max-status-pill failed">Halted — a task failed</span>
          ) : (
            <span className="max-status-pill pending">{total} tasks</span>
          )}
        </div>
        <span className="max-progress">{completed}/{total} done</span>
      </header>

      <ol className="max-task-list">
        {list.map((task) => {
          const { summary, details } = splitTaskDescription(task.description);
          return (
            <li className={`max-task-item ${task.status}`} key={task.id}>
              <span className="max-task-status" aria-label={task.status}>
                {task.status === "completed"
                  ? "✓"
                  : task.status === "failed"
                  ? "✕"
                  : task.status === "paused"
                  ? "⏸"
                  : task.status === "running"
                  ? "◐"
                  : "○"}
              </span>
              <div className="max-task-text">
                <span className="max-task-title">{task.title}</span>
                {summary && <span className="max-task-desc">{summary}</span>}
                {details && (
                  <details className="max-task-details">
                    <summary>Details</summary>
                    <pre className="max-task-detail-body">{details}</pre>
                  </details>
                )}
              </div>
              <span className={`max-task-pill ${task.status}`}>{statusLabel(task.status)}</span>
            </li>
          );
        })}
        {!list.length && <li className="max-task-empty">No tasks were generated.</li>}
      </ol>

      {plan && (
        <details className="max-plan-details">
          <summary>View full plan</summary>
          <pre className="max-plan-body">{plan}</pre>
        </details>
      )}

      <div className="max-card-actions">
        {hasPaused ? (
          <>
            <button className="max-run-btn" type="button" disabled={isSending} onClick={onRun}>
              <span aria-hidden="true">▶</span> Continue Task
            </button>
            {onStopTask && pausedTask && (
              <button
                className="max-run-btn danger"
                type="button"
                disabled={isSending}
                onClick={() => onStopTask(pausedTask.id)}
              >
                <span aria-hidden="true">■</span> Stop
              </button>
            )}
          </>
        ) : hasFailed ? (
          <>
            <button className="max-run-btn" type="button" disabled={isSending} onClick={onRetryFailed}>
              <span aria-hidden="true">↻</span> Retry failed task{list.filter((task) => task.status === "failed").length > 1 ? "s" : ""}
            </button>
            <button className="max-run-btn secondary" type="button" disabled={isSending || !hasPending} onClick={onRun}>
              <span aria-hidden="true">▶</span> Resume remaining
            </button>
          </>
        ) : (
          <button className="max-run-btn" type="button" disabled={isSending || isRunning || !hasPending} onClick={onRun}>
            <span aria-hidden="true">▶</span>
            {completed > 0 ? "Resume tasks" : "Run all tasks"}
          </button>
        )}
      </div>
    </article>
  );
}
