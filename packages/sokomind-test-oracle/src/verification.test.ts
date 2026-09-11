import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { bfsSolve } from "./bfs-oracle.ts";
import type { Direction } from "./bfs-oracle.ts";

interface BoardCell {
  row: number;
  col: number;
}

interface ParsedBoard {
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

function offsetPos(pos: BoardCell, dir: Direction): BoardCell {
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

function parseBoardFromRows(rows: string[]): ParsedBoard {
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
        goals.push({ position: { row: r, col: c }, label: "X" });
      } else if (ch >= "A" && ch <= "Z" && !"ORSX".includes(ch)) {
        initialBoxes.push({
          id: `box-${boxId++}`,
          label: ch,
          position: { row: r, col: c },
        });
      } else if (ch >= "a" && ch <= "z") {
        goals.push({ position: { row: r, col: c }, label: ch.toUpperCase() });
      }
    }
  }

  return { walls, goals, initialRobot, initialBoxes };
}

function stepReplay(
  board: ParsedBoard,
  state: ReplayState,
  dir: Direction,
): { state: ReplayState; moved: boolean; pushed: boolean } {
  const target = offsetPos(state.robot, dir);

  if (board.walls.has(cellKey(target))) {
    return { state, moved: false, pushed: false };
  }

  const boxIdx = state.boxes.findIndex(
    (b) => b.position.row === target.row && b.position.col === target.col,
  );

  if (boxIdx >= 0) {
    const pushTarget = offsetPos(target, dir);
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

function replayPath(
  rows: string[],
  path: Direction[],
): { valid: boolean; moves: number; pushes: number; solved: boolean } {
  const board = parseBoardFromRows(rows);
  let state: ReplayState = {
    robot: board.initialRobot,
    boxes: board.initialBoxes.map((b) => ({ ...b })),
    moves: 0,
    pushes: 0,
  };

  for (const dir of path) {
    const { state: next, moved } = stepReplay(board, state, dir);
    if (!moved) {
      return { valid: false, moves: state.moves, pushes: state.pushes, solved: false };
    }
    state = next;
  }

  return {
    valid: true,
    moves: state.moves,
    pushes: state.pushes,
    solved: checkSolved(board, state.boxes),
  };
}

describe("cross-engine verification: BFS oracle paths replay correctly", () => {
  const fixtures: Array<{ name: string; rows: string[]; expectedPushes: number }> = [
    {
      name: "trivial-1box",
      rows: ["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"],
      expectedPushes: 2,
    },
    {
      name: "trivial-1box-short",
      rows: ["OOOOO", "OSXRO", "OOOOO"],
      expectedPushes: 1,
    },
    {
      name: "two-box-horizontal",
      rows: ["OOOOOOO", "O S S O", "O     O", "O X X O", "O  R  O", "OOOOOOO"],
      expectedPushes: 4,
    },
    {
      name: "typed-2box",
      rows: ["OOOOOOO", "O a   O", "O AR  O", "O B   O", "O   b O", "OOOOOOO"],
      expectedPushes: 4,
    },
    {
      name: "l-push",
      rows: ["OOOOO", "O  SO", "O X O", "O R O", "OOOOO"],
      expectedPushes: 2,
    },
  ];

  for (const fixture of fixtures) {
    it(`${fixture.name}: oracle solution replays to solved state`, () => {
      const oracle = bfsSolve(fixture.rows);
      assert.ok(oracle.solvable, `${fixture.name}: oracle should find solution`);

      const replay = replayPath(fixture.rows, oracle.path);
      assert.ok(replay.valid, `${fixture.name}: all moves should be valid`);
      assert.ok(replay.solved, `${fixture.name}: replay should reach solved state`);
      assert.equal(
        replay.pushes,
        oracle.pushes,
        `${fixture.name}: replay push count should match oracle`,
      );
      assert.equal(
        replay.moves,
        oracle.moves,
        `${fixture.name}: replay move count should match oracle`,
      );
    });

    it(`${fixture.name}: oracle finds optimal push count`, () => {
      const oracle = bfsSolve(fixture.rows);
      assert.ok(oracle.solvable);
      assert.equal(
        oracle.pushes,
        fixture.expectedPushes,
        `${fixture.name}: expected ${fixture.expectedPushes} pushes, got ${oracle.pushes}`,
      );
    });
  }
});

describe("replay engine edge cases", () => {
  it("detects blocked move", () => {
    const rows = ["OOO", "ORO", "OOO"];
    const board = parseBoardFromRows(rows);
    const state: ReplayState = {
      robot: board.initialRobot,
      boxes: [],
      moves: 0,
      pushes: 0,
    };
    const { moved } = stepReplay(board, state, "Up");
    assert.ok(!moved);
  });

  it("detects box-into-wall", () => {
    const rows = ["OOOOO", "ORXOO", "OOOOO"];
    const board = parseBoardFromRows(rows);
    const state: ReplayState = {
      robot: board.initialRobot,
      boxes: board.initialBoxes.map((b) => ({ ...b })),
      moves: 0,
      pushes: 0,
    };
    const { moved } = stepReplay(board, state, "Right");
    assert.ok(!moved);
  });

  it("detects box-into-box", () => {
    const rows = ["OOOOOOO", "ORXX  O", "OOOOOOO"];
    const board = parseBoardFromRows(rows);
    const state: ReplayState = {
      robot: board.initialRobot,
      boxes: board.initialBoxes.map((b) => ({ ...b })),
      moves: 0,
      pushes: 0,
    };
    const { moved } = stepReplay(board, state, "Right");
    assert.ok(!moved);
  });

  it("correctly parses typed boxes and goals", () => {
    const rows = ["OOOOOOO", "O a   O", "O AR  O", "O B   O", "O   b O", "OOOOOOO"];
    const board = parseBoardFromRows(rows);
    assert.equal(board.initialBoxes.length, 2);
    assert.equal(board.goals.length, 2);
    const labels = board.initialBoxes.map((b) => b.label).sort();
    assert.deepEqual(labels, ["A", "B"]);
    const goalLabels = board.goals.map((g) => g.label).sort();
    assert.deepEqual(goalLabels, ["A", "B"]);
  });

  it("robot can walk freely on empty floor", () => {
    const rows = ["OOOOO", "O   O", "O R O", "O   O", "OOOOO"];
    const board = parseBoardFromRows(rows);
    let state: ReplayState = {
      robot: board.initialRobot,
      boxes: [],
      moves: 0,
      pushes: 0,
    };
    for (const dir of ["Up", "Left", "Down", "Right"] as Direction[]) {
      const { state: next, moved } = stepReplay(board, state, dir);
      assert.ok(moved, `should be able to move ${dir}`);
      state = next;
    }
    assert.equal(state.moves, 4);
    assert.equal(state.pushes, 0);
  });
});
