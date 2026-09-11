import type { WorkerEnvelope } from "./protocol.ts";
import type {
  ProgressUpdate,
  SolverOptions,
  SolverResult,
} from "./types.ts";

export interface SolveCallbacks {
  onProgress?: (update: ProgressUpdate) => void;
  onResult: (result: SolverResult) => void;
  onError?: (message: string) => void;
}

export class SolverWorkerClient {
  private worker: Worker;
  private callbacks: SolveCallbacks | null = null;
  private solving = false;

  constructor() {
    this.worker = new Worker(
      new URL("./worker.ts", import.meta.url),
      { type: "module" },
    );
    this.worker.onmessage = this.handleMessage.bind(this);
    this.worker.onerror = this.handleError.bind(this);
  }

  solve(
    puzzleRows: string[],
    options: Partial<SolverOptions>,
    callbacks: SolveCallbacks,
  ): void {
    if (this.solving) {
      callbacks.onError?.("solve already in progress");
      return;
    }

    this.solving = true;
    this.callbacks = callbacks;

    this.worker.postMessage({
      type: "Solve",
      puzzle_json: JSON.stringify({ rows: puzzleRows }),
      options_json: JSON.stringify({
        mode: options.mode ?? "Fast",
        deterministic: options.deterministic ?? false,
        log_level: options.log_level ?? "Info",
        max_time_ms: options.max_time_ms ?? 180_000,
        max_expanded_states: options.max_expanded_states ?? 500_000,
        max_memory_bytes: options.max_memory_bytes ?? 768 * 1024 * 1024,
        seed: options.seed,
      }),
    });
  }

  cancel(): void {
    if (!this.solving) return;
    this.worker.postMessage({ type: "Cancel" });
  }

  async ping(): Promise<string> {
    return new Promise((resolve) => {
      const handler = (event: MessageEvent<WorkerEnvelope>) => {
        const { payload } = event.data;
        if (payload.type === "Pong") {
          this.worker.removeEventListener("message", handler);
          resolve(payload.version);
        }
      };
      this.worker.addEventListener("message", handler);
      this.worker.postMessage({ type: "Ping" });
    });
  }

  get isSolving(): boolean {
    return this.solving;
  }

  terminate(): void {
    this.worker.terminate();
  }

  private handleMessage(event: MessageEvent<WorkerEnvelope>): void {
    const { payload } = event.data;

    switch (payload.type) {
      case "Progress":
        this.callbacks?.onProgress?.({
          phase: payload.phase,
          elapsed_ms: payload.elapsed_ms,
          expanded_states: payload.expanded_states,
          generated_states: payload.generated_states,
          best_pushes: payload.best_pushes,
          best_moves: payload.best_moves,
          log_entries: [],
        });
        break;

      case "Result":
        this.solving = false;
        try {
          const result: SolverResult = JSON.parse(payload.result_json);
          this.callbacks?.onResult(result);
        } catch (e: unknown) {
          const msg = e instanceof Error ? e.message : String(e);
          this.callbacks?.onError?.(`failed to parse result: ${msg}`);
        }
        this.callbacks = null;
        break;

      case "Error":
        this.solving = false;
        this.callbacks?.onError?.(payload.message);
        this.callbacks = null;
        break;

      case "Pong":
        break;
    }
  }

  private handleError(event: ErrorEvent): void {
    this.solving = false;
    this.callbacks?.onError?.(event.message);
    this.callbacks = null;
  }
}
