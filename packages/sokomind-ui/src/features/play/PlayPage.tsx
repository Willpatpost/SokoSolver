import { useCallback, useEffect, useReducer, useRef, useState } from "react";
import type { Direction, GameSnapshot, ParsedBoard, PuzzleDefinition } from "../../core/types.ts";
import { createSnapshot, parsePuzzle, stepSnapshot } from "../../core/engine.ts";
import { getOrderedPuzzles } from "../../catalog/puzzles.ts";
import { SolverWorkerClient } from "../../solver/worker-client.ts";
import type { SolverResult, SolverPhase } from "../../solver/types.ts";
import { isSolved as isSolverSolved, isUnsolved } from "../../solver/types.ts";
import { Board } from "../game/Board.tsx";
import styles from "./PlayPage.module.css";

interface GameState {
  puzzle: PuzzleDefinition;
  board: ParsedBoard;
  snapshot: GameSnapshot;
  history: GameSnapshot[];
}

type GameAction =
  | { type: "move"; direction: Direction }
  | { type: "undo" }
  | { type: "reset" }
  | { type: "load"; puzzle: PuzzleDefinition };

function initGame(puzzle: PuzzleDefinition): GameState {
  const board = parsePuzzle(puzzle);
  const snapshot = createSnapshot(
    puzzle.id,
    board,
    board.initialRobot,
    board.initialBoxes,
    0,
    0,
  );
  return { puzzle, board, snapshot, history: [] };
}

function gameReducer(state: GameState, action: GameAction): GameState {
  switch (action.type) {
    case "move": {
      if (state.snapshot.solved) return state;
      const result = stepSnapshot(state.board, state.snapshot, action.direction);
      if (!result.moved) return state;
      return {
        ...state,
        snapshot: result.snapshot,
        history: [...state.history, state.snapshot],
      };
    }
    case "undo": {
      if (state.history.length === 0) return state;
      return {
        ...state,
        snapshot: state.history[state.history.length - 1]!,
        history: state.history.slice(0, -1),
      };
    }
    case "reset":
      return initGame(state.puzzle);
    case "load":
      return initGame(action.puzzle);
  }
}

const KEY_MAP: Record<string, Direction> = {
  ArrowUp: "up",
  ArrowDown: "down",
  ArrowLeft: "left",
  ArrowRight: "right",
};

const SOLVER_DIR_MAP: Record<string, Direction> = {
  Up: "up",
  Down: "down",
  Left: "left",
  Right: "right",
};

const DIFFICULTY_COLORS: Record<string, string> = {
  tutorial: "var(--sage-500)",
  beginner: "var(--sage-600)",
  intermediate: "var(--blue-500)",
  advanced: "var(--amber-500)",
  expert: "var(--coral-500)",
  master: "var(--ink-700)",
};

type SolverStatus = "idle" | "solving" | "solved" | "error" | "no-solution";

interface SolverProgress {
  phase: SolverPhase;
  expanded: number;
}

export function PlayPage() {
  const puzzles = getOrderedPuzzles();
  const [state, dispatch] = useReducer(gameReducer, puzzles[0]!, initGame);
  const [showHint, setShowHint] = useState(false);
  const mainRef = useRef<HTMLDivElement>(null);

  const workerRef = useRef<SolverWorkerClient | null>(null);
  const [solverStatus, setSolverStatus] = useState<SolverStatus>("idle");
  const [solverResult, setSolverResult] = useState<SolverResult | null>(null);
  const [solverProgress, setSolverProgress] = useState<SolverProgress | null>(null);
  const [solverError, setSolverError] = useState<string | null>(null);
  const [replaying, setReplaying] = useState(false);
  const replayRef = useRef<number | null>(null);
  const replayStepRef = useRef(0);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      const dir = KEY_MAP[e.key];
      if (dir) {
        e.preventDefault();
        dispatch({ type: "move", direction: dir });
        return;
      }
      if (e.key === "z" || e.key === "Z") {
        e.preventDefault();
        dispatch({ type: "undo" });
        return;
      }
      if (e.key === "r" || e.key === "R") {
        e.preventDefault();
        dispatch({ type: "reset" });
      }
    },
    [],
  );

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  const stopReplay = useCallback(() => {
    if (replayRef.current !== null) {
      clearInterval(replayRef.current);
      replayRef.current = null;
    }
    setReplaying(false);
    replayStepRef.current = 0;
  }, []);

  const selectPuzzle = useCallback((puzzle: PuzzleDefinition) => {
    if (workerRef.current?.isSolving) {
      workerRef.current.cancel();
    }
    dispatch({ type: "load", puzzle });
    setShowHint(false);
    setSolverStatus("idle");
    setSolverResult(null);
    setSolverProgress(null);
    setSolverError(null);
    if (replayRef.current !== null) {
      clearInterval(replayRef.current);
      replayRef.current = null;
    }
    setReplaying(false);
    replayStepRef.current = 0;
    mainRef.current?.focus();
  }, []);

  const handleSolve = useCallback(() => {
    if (!workerRef.current) {
      workerRef.current = new SolverWorkerClient();
    }
    const client = workerRef.current;
    if (client.isSolving) return;

    dispatch({ type: "reset" });
    setSolverStatus("solving");
    setSolverResult(null);
    setSolverProgress(null);
    setSolverError(null);
    stopReplay();

    client.solve(
      [...state.puzzle.rows],
      { mode: "Fast", max_time_ms: 30_000 },
      {
        onProgress(update) {
          setSolverProgress({
            phase: update.phase,
            expanded: update.expanded_states,
          });
        },
        onResult(result) {
          setSolverResult(result);
          if (isSolverSolved(result.status) && result.solution) {
            setSolverStatus("solved");
          } else {
            const reason = isUnsolved(result.status)
              ? result.status.Unsolved.reason
              : `status: ${JSON.stringify(result.status)}`;
            setSolverStatus("no-solution");
            setSolverError(reason);
          }
        },
        onError(message) {
          setSolverStatus("error");
          setSolverError(message);
        },
      },
    );
  }, [state.puzzle, stopReplay]);

  const handleCancel = useCallback(() => {
    workerRef.current?.cancel();
    setSolverStatus("idle");
    setSolverProgress(null);
  }, []);

  const handleReplay = useCallback(() => {
    if (!solverResult?.solution) return;
    const steps = solverResult.solution.steps;

    dispatch({ type: "reset" });
    replayStepRef.current = 0;
    setReplaying(true);

    replayRef.current = window.setInterval(() => {
      const idx = replayStepRef.current;
      if (idx >= steps.length) {
        clearInterval(replayRef.current!);
        replayRef.current = null;
        setReplaying(false);
        return;
      }
      const step = steps[idx]!;
      const dir = SOLVER_DIR_MAP[step.direction];
      if (dir) {
        dispatch({ type: "move", direction: dir });
      }
      replayStepRef.current = idx + 1;
    }, 250);
  }, [solverResult]);

  useEffect(() => {
    return () => {
      if (replayRef.current !== null) clearInterval(replayRef.current);
      workerRef.current?.terminate();
    };
  }, []);

  let currentDifficulty = "";

  return (
    <div className={styles.layout}>
      <nav className={styles.sidebar} aria-label="Puzzle list">
        <div className={styles.sidebarHeader}>Puzzles</div>
        {puzzles.map((p) => {
          const showLabel = p.difficulty !== currentDifficulty;
          if (showLabel) currentDifficulty = p.difficulty;
          return (
            <div key={p.id} className={showLabel ? styles.difficultyGroup : undefined}>
              {showLabel && (
                <div
                  className={styles.difficultyLabel}
                  style={{ color: DIFFICULTY_COLORS[p.difficulty] }}
                >
                  {p.difficulty}
                </div>
              )}
              <button
                className={styles.puzzleButton}
                data-active={p.id === state.puzzle.id || undefined}
                onClick={() => selectPuzzle(p)}
              >
                {p.title}
                <span className={styles.boxCount}>{p.boxes}</span>
              </button>
            </div>
          );
        })}
      </nav>

      <main className={styles.main} ref={mainRef} tabIndex={-1}>
        <div className={styles.boardArea}>
          <Board board={state.board} snapshot={state.snapshot} />
        </div>

        <div className={styles.controls}>
          <div className={styles.stat}>
            <span className={styles.statLabel}>Moves</span>
            <span className={styles.statValue}>{state.snapshot.moves}</span>
          </div>
          <div className={styles.stat}>
            <span className={styles.statLabel}>Pushes</span>
            <span className={styles.statValue}>{state.snapshot.pushes}</span>
          </div>
          <button
            className={styles.controlButton}
            onClick={() => dispatch({ type: "undo" })}
            disabled={state.history.length === 0 || replaying}
          >
            Undo
          </button>
          <button
            className={styles.controlButton}
            onClick={() => dispatch({ type: "reset" })}
            disabled={state.snapshot.moves === 0 || replaying}
          >
            Reset
          </button>
          {state.puzzle.hint && (
            <button
              className={styles.controlButton}
              onClick={() => setShowHint((h) => !h)}
            >
              {showHint ? "Hide Hint" : "Hint"}
            </button>
          )}

          {solverStatus === "idle" && !state.snapshot.solved && (
            <button
              className={`${styles.controlButton} ${styles.solveButton}`}
              onClick={handleSolve}
            >
              Solve
            </button>
          )}
          {solverStatus === "solving" && (
            <button
              className={`${styles.controlButton} ${styles.cancelButton}`}
              onClick={handleCancel}
            >
              Cancel
            </button>
          )}
          {solverStatus === "solved" && !replaying && (
            <button
              className={`${styles.controlButton} ${styles.solveButton}`}
              onClick={handleReplay}
            >
              Replay
            </button>
          )}
          {replaying && (
            <button
              className={`${styles.controlButton} ${styles.cancelButton}`}
              onClick={stopReplay}
            >
              Stop
            </button>
          )}
        </div>

        {state.snapshot.solved && (
          <div className={styles.solvedBanner}>
            Solved in {state.snapshot.moves} moves, {state.snapshot.pushes} pushes
          </div>
        )}

        {solverStatus === "solving" && solverProgress && (
          <div className={styles.solverStatus}>
            {solverProgress.phase} &middot; {solverProgress.expanded.toLocaleString()} states
          </div>
        )}

        {solverStatus === "solved" && solverResult?.solution && !replaying && !state.snapshot.solved && (
          <div className={styles.solverStatus}>
            Found: {solverResult.solution.moves} moves, {solverResult.solution.pushes} pushes
            {solverResult.metrics ? ` (${(solverResult.metrics.elapsed_ms / 1000).toFixed(1)}s)` : ""}
          </div>
        )}

        {solverStatus === "no-solution" && (
          <div className={styles.solverError}>
            No solution found{solverError ? `: ${solverError}` : ""}
          </div>
        )}

        {solverStatus === "error" && (
          <div className={styles.solverError}>
            {solverError ?? "Solver error"}
          </div>
        )}

        {showHint && state.puzzle.hint && !state.snapshot.solved && (
          <div className={styles.hint}>{state.puzzle.hint}</div>
        )}

        <div className={styles.shortcuts}>
          <kbd>Arrow keys</kbd> move &middot; <kbd>Z</kbd> undo &middot; <kbd>R</kbd> reset
        </div>
      </main>
    </div>
  );
}
