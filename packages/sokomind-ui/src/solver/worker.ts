import type { WorkerCommand, WorkerEnvelope, WorkerResponse } from "./protocol.ts";
import { PROTOCOL_VERSION } from "./protocol.ts";

let wasmModule: typeof import("./wasm/sokomind_solver.js") | null = null;
let cancelled = false;

function sendResponse(payload: WorkerResponse): void {
  const envelope: WorkerEnvelope = {
    version: PROTOCOL_VERSION,
    payload,
  };
  self.postMessage(envelope);
}

async function ensureWasm(): Promise<typeof import("./wasm/sokomind_solver.js")> {
  if (wasmModule) return wasmModule;
  const mod = await import("./wasm/sokomind_solver.js");
  await mod.default();
  wasmModule = mod;
  return mod;
}

function handleSolve(puzzleJson: string, optionsJson: string): void {
  cancelled = false;

  ensureWasm()
    .then((wasm) => {
      const onProgress = (progressJson: string): boolean => {
        if (cancelled) return false;

        try {
          const update = JSON.parse(progressJson);
          sendResponse({
            type: "Progress",
            phase: update.phase,
            elapsed_ms: update.elapsed_ms,
            expanded_states: update.expanded_states,
            generated_states: update.generated_states,
            best_pushes: update.best_pushes,
            best_moves: update.best_moves,
          });
        } catch {
          // Progress parse failure is non-fatal
        }

        return !cancelled;
      };

      const resultJson = wasm.solve_with_progress(
        puzzleJson,
        optionsJson,
        onProgress,
      );
      sendResponse({ type: "Result", result_json: resultJson });
    })
    .catch((err: unknown) => {
      const message = err instanceof Error ? err.message : String(err);
      sendResponse({ type: "Error", message });
    });
}

self.onmessage = (event: MessageEvent<WorkerCommand>) => {
  const command = event.data;

  switch (command.type) {
    case "Solve":
      handleSolve(command.puzzle_json, command.options_json);
      break;

    case "Cancel":
      cancelled = true;
      break;

    case "Ping":
      if (wasmModule) {
        sendResponse({
          type: "Pong",
          version: wasmModule.solver_version(),
        });
      } else {
        sendResponse({ type: "Pong", version: "not loaded" });
      }
      break;
  }
};
