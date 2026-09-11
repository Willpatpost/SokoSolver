import { useCallback, useEffect, useReducer, useRef, useState } from "react";
import type { Direction, GameSnapshot, ParsedBoard, PuzzleDefinition } from "../../core/types.ts";
import { createSnapshot, parsePuzzle, stepSnapshot } from "../../core/engine.ts";
import { getOrderedPuzzles } from "../../catalog/puzzles.ts";
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

const DIFFICULTY_COLORS: Record<string, string> = {
  tutorial: "var(--sage-500)",
  beginner: "var(--sage-600)",
  intermediate: "var(--blue-500)",
  advanced: "var(--amber-500)",
  expert: "var(--coral-500)",
  master: "var(--ink-700)",
};

export function PlayPage() {
  const puzzles = getOrderedPuzzles();
  const [state, dispatch] = useReducer(gameReducer, puzzles[0]!, initGame);
  const [showHint, setShowHint] = useState(false);
  const mainRef = useRef<HTMLDivElement>(null);

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

  const selectPuzzle = useCallback((puzzle: PuzzleDefinition) => {
    dispatch({ type: "load", puzzle });
    setShowHint(false);
    mainRef.current?.focus();
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
            disabled={state.history.length === 0}
          >
            Undo
          </button>
          <button
            className={styles.controlButton}
            onClick={() => dispatch({ type: "reset" })}
            disabled={state.snapshot.moves === 0}
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
        </div>

        {state.snapshot.solved && (
          <div className={styles.solvedBanner}>
            Solved in {state.snapshot.moves} moves, {state.snapshot.pushes} pushes
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
