import type { Direction, SolutionStep, SolverResult } from "./types.ts";
import { isSolved } from "./types.ts";

export interface BoardCell {
  row: number;
  col: number;
}

export interface ParsedBoard {
  walls: Set<string>;
  goals: Array<{ position: BoardCell; label: string }>;
  initialRobot: BoardCell;
  initialBoxes: Array<{ id: string; label: string; position: BoardCell }>;
}

interface ReplayState {
  robot: BoardCell;
  boxes: Array<{ id: string; label: string; position: BoardCell }>;
  moves: number;
  pushes: number;
}

function cellKey(c: BoardCell): string {
  return `${c.row},${c.col}`;
}

function offset(pos: BoardCell, dir: Direction): BoardCell {
  switch (dir) {
    case "Up":
      return { row: pos.row - 1, col: pos.col };
    case "Down":
      return { row: pos.row + 1, col: pos.col };
    case "Left":
      return { row: pos.row, col: pos.col - 1 };
    case "Right":
      return { row: pos.row, col: pos.col + 1 };
  }
}

function stepReplay(
  board: ParsedBoard,
  state: ReplayState,
  dir: Direction,
): { state: ReplayState; moved: boolean; pushed: boolean } {
  const target = offset(state.robot, dir);

  if (board.walls.has(cellKey(target))) {
    return { state, moved: false, pushed: false };
  }

  const boxIdx = state.boxes.findIndex(
    (b) => b.position.row === target.row && b.position.col === target.col,
  );

  if (boxIdx >= 0) {
    const pushTarget = offset(target, dir);

    if (board.walls.has(cellKey(pushTarget))) {
      return { state, moved: false, pushed: false };
    }
    if (
      state.boxes.some(
        (b) =>
          b.position.row === pushTarget.row &&
          b.position.col === pushTarget.col,
      )
    ) {
      return { state, moved: false, pushed: false };
    }

    const newBoxes = state.boxes.map((b, i) =>
      i === boxIdx ? { ...b, position: pushTarget } : { ...b },
    );

    return {
      state: {
        robot: target,
        boxes: newBoxes,
        moves: state.moves + 1,
        pushes: state.pushes + 1,
      },
      moved: true,
      pushed: true,
    };
  }

  return {
    state: {
      robot: target,
      boxes: state.boxes,
      moves: state.moves + 1,
      pushes: state.pushes,
    },
    moved: true,
    pushed: false,
  };
}

function checkSolved(board: ParsedBoard, boxes: ReplayState["boxes"]): boolean {
  return boxes.every((box) =>
    board.goals.some(
      (g) =>
        g.position.row === box.position.row &&
        g.position.col === box.position.col &&
        g.label === box.label,
    ),
  );
}

export function parseBoardFromRows(rows: string[]): ParsedBoard {
  const walls = new Set<string>();
  const goals: ParsedBoard["goals"] = [];
  const initialBoxes: ParsedBoard["initialBoxes"] = [];
  let initialRobot: BoardCell = { row: 0, col: 0 };
  let boxId = 0;

  for (let r = 0; r < rows.length; r++) {
    const row = rows[r]!;
    for (let c = 0; c < row.length; c++) {
      const ch = row[c]!;
      if (ch === "O") {
        walls.add(cellKey({ row: r, col: c }));
      } else if (ch === "R") {
        initialRobot = { row: r, col: c };
      } else if (ch === "X") {
        initialBoxes.push({
          id: `box-${boxId++}`,
          label: "X",
          position: { row: r, col: c },
        });
      } else if (ch === "S") {
        goals.push({
          position: { row: r, col: c },
          label: "X",
        });
      } else if (ch >= "A" && ch <= "Z" && ch !== "O" && ch !== "R" && ch !== "S" && ch !== "X") {
        initialBoxes.push({
          id: `box-${boxId++}`,
          label: ch,
          position: { row: r, col: c },
        });
      } else if (ch >= "a" && ch <= "z") {
        goals.push({
          position: { row: r, col: c },
          label: ch.toUpperCase(),
        });
      }
    }
  }

  return { walls, goals, initialRobot, initialBoxes };
}

export interface VerificationResult {
  valid: boolean;
  moves: number;
  pushes: number;
  solved: boolean;
  error?: string;
}

export function verifySolution(
  rows: string[],
  steps: SolutionStep[],
): VerificationResult {
  const board = parseBoardFromRows(rows);

  let state: ReplayState = {
    robot: board.initialRobot,
    boxes: board.initialBoxes.map((b) => ({ ...b })),
    moves: 0,
    pushes: 0,
  };

  for (let i = 0; i < steps.length; i++) {
    const step = steps[i]!;
    const { state: next, moved, pushed } = stepReplay(
      board,
      state,
      step.direction,
    );

    if (!moved) {
      return {
        valid: false,
        moves: state.moves,
        pushes: state.pushes,
        solved: false,
        error: `step ${i}: move ${step.direction} is blocked`,
      };
    }

    if (step.pushed && !pushed) {
      return {
        valid: false,
        moves: state.moves,
        pushes: state.pushes,
        solved: false,
        error: `step ${i}: expected push but no box was pushed`,
      };
    }

    state = next;
  }

  const solved = checkSolved(board, state.boxes);

  return {
    valid: true,
    moves: state.moves,
    pushes: state.pushes,
    solved,
    error: solved ? undefined : "replay completed but puzzle not solved",
  };
}

export function verifyResult(
  rows: string[],
  result: SolverResult,
): VerificationResult {
  if (!isSolved(result.status) || !result.solution) {
    return {
      valid: false,
      moves: 0,
      pushes: 0,
      solved: false,
      error: "no solution to verify",
    };
  }

  return verifySolution(rows, result.solution.steps);
}
