import type { SolverPhase } from "./types.ts";

export const PROTOCOL_VERSION = 2;

export type WorkerCommand =
  | { type: "Solve"; puzzle_json: string; options_json: string }
  | { type: "Cancel" }
  | { type: "Ping" };

export type WorkerResponse =
  | {
      type: "Progress";
      phase: SolverPhase;
      elapsed_ms: number;
      expanded_states: number;
      generated_states: number;
      best_pushes: number | null;
      best_moves: number | null;
    }
  | { type: "Result"; result_json: string }
  | { type: "Error"; message: string }
  | { type: "Pong"; version: string };

export interface WorkerEnvelope {
  version: number;
  payload: WorkerResponse;
}

export function createSolveCommand(
  puzzleRows: string[],
  options: {
    mode?: string;
    deterministic?: boolean;
    log_level?: string;
    max_time_ms?: number;
    max_expanded_states?: number;
    max_memory_bytes?: number;
    seed?: number;
  } = {},
): WorkerCommand {
  return {
    type: "Solve",
    puzzle_json: JSON.stringify({ rows: puzzleRows }),
    options_json: JSON.stringify(options),
  };
}
