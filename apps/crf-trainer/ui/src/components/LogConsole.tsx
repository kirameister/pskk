import { useEffect, useRef } from "react";
import type { LogLevel, LogLine } from "../types";

interface LogConsoleProps {
  lines: LogLine[];
  /** Overall progress of the active job, when known. */
  progress: { current: number; total: number } | null;
  running: boolean;
  onClear: () => void;
}

const LEVEL_CLASS: Record<LogLevel, string> = {
  info: "log-info",
  warn: "log-warn",
  error: "log-error",
  success: "log-success",
};

/**
 * Training/extraction log console with in-place progress bar.
 * 訓練・抽出ログコンソール（インプレース進捗バー付き）。
 */
export default function LogConsole({ lines, progress, running, onClear }: LogConsoleProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const pinnedRef = useRef(true);

  // Auto-scroll only while the user is already at the bottom.
  useEffect(() => {
    const node = scrollRef.current;
    if (!node || !pinnedRef.current) return;
    node.scrollTop = node.scrollHeight;
  }, [lines]);

  const percent =
    progress && progress.total > 0
      ? Math.min(100, Math.round((progress.current / progress.total) * 100))
      : null;

  return (
    <section className="panel log-panel">
      <header className="panel-head">
        <h3>Training Log</h3>
        <div className="panel-head-actions">
          {running && <span className="spinner" aria-label="running" />}
          <span className="muted">{lines.length} lines</span>
          <button type="button" className="btn btn-ghost btn-sm" onClick={onClear} disabled={!lines.length}>
            Clear
          </button>
        </div>
      </header>

      {progress && (
        <div className="progress-track" role="progressbar" aria-valuenow={percent ?? undefined}>
          <div
            className="progress-fill"
            style={{ width: percent !== null ? `${percent}%` : "100%" }}
          />
          <span className="progress-label">
            {progress.current} / {progress.total}
            {percent !== null ? ` (${percent}%)` : ""}
          </span>
        </div>
      )}

      <div
        className="log-scroll"
        ref={scrollRef}
        onScroll={(event) => {
          const node = event.currentTarget;
          pinnedRef.current = node.scrollHeight - node.scrollTop - node.clientHeight < 24;
        }}
      >
        {lines.length === 0 ? (
          <p className="empty-note">No log output yet.</p>
        ) : (
          lines.map((line, index) => (
            <div key={`${line.timestamp}-${index}`} className={`log-line ${LEVEL_CLASS[line.level]}`}>
              <span className="log-time">{line.timestamp}</span>
              <span className="log-stage">{line.stage}</span>
              <span className="log-message">{line.message}</span>
            </div>
          ))
        )}
      </div>
    </section>
  );
}
