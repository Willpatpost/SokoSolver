import { useEffect, useRef, useState } from "react";
import type {
  LogEntry,
  PhaseReport,
  SolverResult,
  TelemetryCounters,
} from "../../solver/types.ts";
import { isSolved, isUnsolved } from "../../solver/types.ts";
import styles from "./SolveLog.module.css";

interface SolveLogProps {
  logEntries: LogEntry[];
  solverResult: SolverResult | null;
  solving: boolean;
  puzzleTitle: string;
}

export function SolveLog({
  logEntries,
  solverResult,
  solving,
  puzzleTitle,
}: SolveLogProps) {
  const [open, setOpen] = useState(false);
  const [copied, setCopied] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);

  const hasContent = logEntries.length > 0 || solverResult !== null;

  useEffect(() => {
    if (open && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [logEntries.length, open]);

  useEffect(() => {
    if (solving) setOpen(true);
  }, [solving]);

  const handleCopy = () => {
    const text = formatFullReport(puzzleTitle, logEntries, solverResult);
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  };

  if (!hasContent && !solving) return null;

  const badgeText = solving
    ? "solving..."
    : solverResult
      ? isSolved(solverResult.status)
        ? "solved"
        : "done"
      : "";

  const badgeClass = solving
    ? `${styles.badge} ${styles.badgeSolving}`
    : solverResult && !isSolved(solverResult.status)
      ? `${styles.badge} ${styles.badgeError}`
      : styles.badge;

  return (
    <div className={styles.container}>
      <button className={styles.toggle} onClick={() => setOpen(!open)}>
        <span className={`${styles.arrow} ${open ? styles.arrowOpen : ""}`}>
          {"▶"}
        </span>
        Solver Log
        {badgeText && <span className={badgeClass}>{badgeText}</span>}
      </button>

      {open && (
        <div className={styles.panel}>
          <div className={styles.toolbar}>
            <span style={{ fontSize: "0.72rem", color: "var(--ink-muted)" }}>
              {logEntries.length} entries
            </span>
            <button
              className={`${styles.copyButton} ${copied ? styles.copied : ""}`}
              onClick={handleCopy}
            >
              {copied ? "Copied" : "Copy Log"}
            </button>
          </div>

          <div className={styles.logContent} ref={scrollRef}>
            {solverResult && <ResultSection result={solverResult} />}
            {solverResult && solverResult.phase_reports.length > 0 && (
              <PhaseSection reports={solverResult.phase_reports} />
            )}
            {solverResult && <TelemetrySection telemetry={solverResult.telemetry} />}
            {solverResult?.proof && (
              <div className={styles.section}>
                <div className={styles.sectionHeader}>Proof</div>
                <pre className={styles.pre}>
                  {`Kind: ${solverResult.proof.kind}, lower_bound=${solverResult.proof.lower_bound}, upper_bound=${solverResult.proof.upper_bound}, gap=${solverResult.proof.gap}`}
                </pre>
              </div>
            )}
            {logEntries.length > 0 && <LogSection entries={logEntries} />}
          </div>
        </div>
      )}
    </div>
  );
}

function ResultSection({ result }: { result: SolverResult }) {
  const status = isSolved(result.status)
    ? "SOLVED"
    : isUnsolved(result.status)
      ? "UNSOLVED"
      : "CANCELLED";

  const statusClass =
    status === "SOLVED" ? styles.statusSolved : styles.statusFailed;

  return (
    <div className={styles.section}>
      <div className={styles.sectionHeader}>Result</div>
      <pre className={styles.pre}>
        <span className={statusClass}>{status}</span>
        {` in ${result.metrics.elapsed_ms.toFixed(1)}ms\n`}
        {result.solution &&
          `Solution: ${result.solution.moves} moves, ${result.solution.pushes} pushes\n`}
        {`Expanded: ${result.metrics.expanded_states.toLocaleString()} | Generated: ${result.metrics.generated_states.toLocaleString()} | Peak frontier: ${result.metrics.peak_frontier.toLocaleString()}`}
        {result.metrics.deadlock_prunes > 0 &&
          `\nDeadlock prunes: ${result.metrics.deadlock_prunes.toLocaleString()}`}
        {isUnsolved(result.status) &&
          `\nReason: ${result.status.Unsolved.reason}`}
      </pre>
    </div>
  );
}

function PhaseSection({ reports }: { reports: PhaseReport[] }) {
  return (
    <div className={styles.section}>
      <div className={styles.sectionHeader}>Phase Reports</div>
      <pre className={styles.pre}>
        {reports.map((r, i) => formatPhaseReport(r, i)).join("\n")}
      </pre>
    </div>
  );
}

function formatPhaseReport(report: PhaseReport, _index: number): string {
  const parts = [`${report.phase}: ${report.elapsed_ms.toFixed(1)}ms`];
  const entries = Object.entries(report.counters)
    .filter(([, v]) => v !== 0)
    .sort(([a], [b]) => a.localeCompare(b));
  for (const [key, val] of entries) {
    parts.push(
      `${key}=${Number.isInteger(val) ? val.toLocaleString() : val.toFixed(2)}`,
    );
  }
  let line = parts.join(", ");
  for (const sub of report.sub_reports) {
    line += "\n  " + formatPhaseReport(sub, 0);
  }
  return line;
}

function TelemetrySection({ telemetry }: { telemetry: TelemetryCounters }) {
  const entries = Object.entries(telemetry)
    .filter(([, v]) => typeof v === "number" && v !== 0)
    .sort(([a], [b]) => a.localeCompare(b));

  if (entries.length === 0) return null;

  return (
    <div className={styles.section}>
      <div className={styles.sectionHeader}>Telemetry</div>
      <pre className={styles.pre}>
        {entries
          .map(([k, v]) => `${k}=${(v as number).toLocaleString()}`)
          .join(", ")}
      </pre>
    </div>
  );
}

function LogSection({ entries }: { entries: LogEntry[] }) {
  return (
    <div className={styles.section}>
      <div className={styles.sectionHeader}>Log</div>
      <pre className={styles.pre}>
        {entries.map((e, i) => {
          const levelClass =
            e.level === "Warn"
              ? styles.levelWarn
              : e.level === "Error"
                ? styles.levelError
                : "";
          return (
            <span key={i} className={`${styles.liveEntry} ${levelClass}`}>
              <span className={styles.timestamp}>
                [{e.timestamp_ms.toFixed(1)}ms]
              </span>{" "}
              <span className={styles.phase}>[{e.phase}]</span>{" "}
              {e.span ? `[${e.span}] ` : ""}
              {e.message}
              {"\n"}
            </span>
          );
        })}
      </pre>
    </div>
  );
}

function formatFullReport(
  puzzleTitle: string,
  logEntries: LogEntry[],
  result: SolverResult | null,
): string {
  const lines: string[] = [];
  lines.push("=== Sokomind Solve Report ===");
  lines.push(`Puzzle: ${puzzleTitle}`);

  if (result) {
    const status = isSolved(result.status)
      ? "SOLVED"
      : isUnsolved(result.status)
        ? "UNSOLVED"
        : "CANCELLED";
    lines.push(`Status: ${status}`);

    if (result.solution) {
      lines.push(
        `Solution: ${result.solution.moves} moves, ${result.solution.pushes} pushes`,
      );
    }

    lines.push(`Elapsed: ${result.metrics.elapsed_ms.toFixed(1)}ms`);
    lines.push(
      `Expanded: ${result.metrics.expanded_states} | Generated: ${result.metrics.generated_states} | Peak frontier: ${result.metrics.peak_frontier}`,
    );

    if (result.metrics.deadlock_prunes > 0) {
      lines.push(`Deadlock prunes: ${result.metrics.deadlock_prunes}`);
    }

    if (isUnsolved(result.status)) {
      lines.push(`Reason: ${result.status.Unsolved.reason}`);
    }

    if (result.phase_reports.length > 0) {
      lines.push("");
      lines.push("--- Phase Reports ---");
      for (const report of result.phase_reports) {
        lines.push(formatPhaseReportText(report, 0));
      }
    }

    const telEntries = Object.entries(result.telemetry)
      .filter(([, v]) => typeof v === "number" && v !== 0)
      .sort(([a], [b]) => a.localeCompare(b));
    if (telEntries.length > 0) {
      lines.push("");
      lines.push("--- Telemetry ---");
      lines.push(telEntries.map(([k, v]) => `${k}=${v}`).join(", "));
    }

    if (result.proof) {
      lines.push("");
      lines.push("--- Proof ---");
      lines.push(
        `Kind: ${result.proof.kind}, lower_bound=${result.proof.lower_bound}, upper_bound=${result.proof.upper_bound}, gap=${result.proof.gap}`,
      );
    }
  }

  if (logEntries.length > 0) {
    lines.push("");
    lines.push("--- Log ---");
    for (const e of logEntries) {
      const span = e.span ? ` [${e.span}]` : "";
      lines.push(`[${e.timestamp_ms.toFixed(1)}ms] [${e.phase}]${span} ${e.message}`);
    }
  }

  return lines.join("\n");
}

function formatPhaseReportText(report: PhaseReport, indent: number): string {
  const prefix = "  ".repeat(indent);
  const parts = [`${report.phase}: ${report.elapsed_ms.toFixed(1)}ms`];
  const entries = Object.entries(report.counters)
    .filter(([, v]) => v !== 0)
    .sort(([a], [b]) => a.localeCompare(b));
  for (const [key, val] of entries) {
    parts.push(`${key}=${Number.isInteger(val) ? val : val.toFixed(2)}`);
  }
  let line = prefix + parts.join(", ");
  for (const sub of report.sub_reports) {
    line += "\n" + formatPhaseReportText(sub, indent + 1);
  }
  return line;
}
