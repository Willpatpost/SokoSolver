export type {
  Direction,
  GameSnapshot,
  LogEntry,
  LogLevel,
  PhaseReport,
  Position,
  ProgressUpdate,
  ProofKind,
  Solution,
  SolutionStep,
  SolverMetrics,
  SolverMode,
  SolverOptions,
  SolverPhase,
  SolverProof,
  SolverResult,
  SolverStatus,
  TelemetryCounters,
} from "./types.ts";

export { isCancelled, isSolved, isUnsolved } from "./types.ts";

export type { WorkerCommand, WorkerEnvelope, WorkerResponse } from "./protocol.ts";
export { PROTOCOL_VERSION, createSolveCommand } from "./protocol.ts";

export type { SolveCallbacks } from "./worker-client.ts";
export { SolverWorkerClient } from "./worker-client.ts";

export type { VerificationResult } from "./verification.ts";
export {
  parseBoardFromRows,
  verifyResult,
  verifySolution,
} from "./verification.ts";
