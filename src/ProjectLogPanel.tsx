import { useEffect, useRef, useState } from "react";

/// Mirrors `progress::LogLine`: `channel` picks the tab, `stream` the colour.
export type ProgressLine = {
  seq: number;
  channel: string;
  stream: "info" | "command" | "stdout" | "stderr" | "error" | "done";
  text: string;
};

type Channel = { key: string; label: string };
type ChannelStatus = "waiting" | "running" | "done" | "failed";
type ProjectLogPanelProps = { lines: ProgressLine[] };

// One tab per shell `llm::initialize_project` runs, in the order it runs them.
const channels: Channel[] = [
  { key: "symfony", label: "Backend" },
  { key: "react", label: "Frontend" },
];
const statusMarks: Record<ChannelStatus, string> = { waiting: "·", running: "⟳", done: "✓", failed: "!" };

function statusOf(lines: ProgressLine[]): ChannelStatus {
  if (lines.some(({ stream }) => stream === "error")) return "failed";
  if (lines.some(({ stream }) => stream === "done")) return "done";
  return lines.length > 0 ? "running" : "waiting";
}

export default function ProjectLogPanel({ lines }: ProjectLogPanelProps) {
  const [activeChannel, setActiveChannel] = useState(channels[0].key);
  const bodyRef = useRef<HTMLDivElement | null>(null);
  // Following along is only welcome until the user takes over — picking a tab or
  // scrolling back to read something is a request to be left alone.
  const hasPickedTabRef = useRef(false);
  const isPinnedRef = useRef(true);

  const newestChannel = lines.length > 0 ? lines[lines.length - 1].channel : undefined;

  useEffect(() => {
    if (hasPickedTabRef.current || !newestChannel) return;
    if (channels.some(({ key }) => key === newestChannel)) setActiveChannel(newestChannel);
  }, [newestChannel]);

  useEffect(() => {
    const body = bodyRef.current;
    if (body && isPinnedRef.current) body.scrollTop = body.scrollHeight;
  }, [lines, activeChannel]);

  // Lines from a channel with no tab of its own stay in the terminal rather than
  // being dropped into whichever tab happens to be open.
  const visible = lines.filter(({ channel }) => channel === activeChannel);

  return <div className="log-panel-wrap">
    <div className="log-tabs" role="tablist" aria-label="Setup output">
      {channels.map(({ key, label }) => {
        const status = statusOf(lines.filter(({ channel }) => channel === key));
        return <button
          className={key === activeChannel ? "log-tab active" : "log-tab"}
          key={key} type="button" role="tab" aria-selected={key === activeChannel}
          onClick={() => { hasPickedTabRef.current = true; isPinnedRef.current = true; setActiveChannel(key); }}
        >
          {label}
          <span className={`tab-status ${status}`} aria-label={status}>{statusMarks[status]}</span>
        </button>;
      })}
    </div>
    <div
      className="log-panel" role="log" aria-label={`${activeChannel} output`} ref={bodyRef}
      onScroll={({ currentTarget }) => {
        const { scrollTop, scrollHeight, clientHeight } = currentTarget;
        isPinnedRef.current = scrollHeight - scrollTop - clientHeight < 24;
      }}
    >
      {visible.length === 0
        ? <p className="log-line info">{activeChannel === channels[0].key ? "Starting…" : "Waiting for the backend to finish…"}</p>
        : visible.map(({ seq, stream, text }) => <p className={`log-line ${stream}`} key={seq}>{text}</p>)}
    </div>
  </div>;
}
