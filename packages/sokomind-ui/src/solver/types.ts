export interface Position {
  row: number;
  col: number;
}

export type Direction = "Up" | "Down" | "Left" | "Right";

export interface BoxEntity {
  id: string;
  label: string;
  position: Position;
}

export interface GameSnapshot {
  robot: Position;
  boxes: BoxEntity[];
  moves: number;
  pushes: number;
  solved: boolean;
}

export type SolverMode = "Fast" | "Quality" | "Optimal";

export type LogLevel = "Trace" | "Debug" | "Info" | "Warn" | "Error";

export interface SolverOptions {
  mode: SolverMode;
  deterministic: boolean;
  log_level: LogLevel;
  max_time_ms?: number;
  max_expanded_states?: number;
  max_memory_bytes?: number;
  seed?: number;
}

export interface SolutionStep {
  direction: Direction;
  pushed: boolean;
}

export interface Solution {
  steps: SolutionStep[];
  moves: number;
  pushes: number;
  final_snapshot: GameSnapshot;
}

export interface SolverMetrics {
  elapsed_ms: number;
  expanded_states: number;
  generated_states: number;
  peak_frontier: number;
  peak_memory_bytes: number;
  deadlock_prunes: number;
}

export type ProofKind = "Bounded" | "Optimal" | "Unsolvable";

export interface SolverProof {
  kind: ProofKind;
  lower_bound: number;
  upper_bound: number;
  gap: number;
}

export type SolverStatus =
  | "Solved"
  | { Unsolved: { reason: string } }
  | "Cancelled";

export interface TelemetryCounters {
  expanded_states: number;
  generated_states: number;
  duplicate_states: number;
  peak_frontier: number;
  peak_memory_bytes: number;
  deadlock_static_prunes: number;
  deadlock_two_by_two_prunes: number;
  deadlock_freeze_prunes: number;
  deadlock_pattern_prunes: number;
  deadlock_pi_corral_prunes: number;
  deadlock_table_prunes: number;
  heuristic_calls: number;
  heuristic_cache_hits: number;
  macro_forced_push: number;
  macro_tunnel: number;
  macro_goal: number;
  transposition_unique: number;
  transposition_duplicate: number;
}

export type SolverPhase =
  | "Preparing"
  | "Searching"
  | "Harvesting"
  | "Improving"
  | "Proving"
  | "Verifying";

export interface PhaseReport {
  phase: SolverPhase;
  elapsed_ms: number;
  counters: Record<string, number>;
  sub_reports: PhaseReport[];
}

export interface LogEntry {
  timestamp_ms: number;
  level: LogLevel;
  phase: SolverPhase;
  span: string | null;
  message: string;
  counters: Record<string, number> | null;
}

export interface SolverResult {
  status: SolverStatus;
  solution: Solution | null;
  metrics: SolverMetrics;
  proof: SolverProof | null;
  telemetry: TelemetryCounters;
  phase_reports: PhaseReport[];
}

export interface ProgressUpdate {
  phase: SolverPhase;
  elapsed_ms: number;
  expanded_states: number;
  generated_states: number;
  best_pushes: number | null;
  best_moves: number | null;
  log_entries: LogEntry[];
}

export function isSolved(status: SolverStatus): status is "Solved" {
  return status === "Solved";
}

export function isUnsolved(
  status: SolverStatus,
): status is { Unsolved: { reason: string } } {
  return typeof status === "object" && "Unsolved" in status;
}

export function isCancelled(status: SolverStatus): status is "Cancelled" {
  return status === "Cancelled";
}
