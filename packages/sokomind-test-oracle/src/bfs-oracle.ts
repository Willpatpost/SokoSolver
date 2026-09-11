export interface Position {
  row: number;
  col: number;
}

export type Direction = "Up" | "Down" | "Left" | "Right";

const DIRECTIONS: Direction[] = ["Up", "Down", "Left", "Right"];

interface BoardState {
  walls: Set<string>;
  goals: Array<{ position: Position; label: string }>;
}

interface SearchState {
  robot: Position;
  boxes: Array<{ position: Position; label: string }>;
}

function cellKey(p: Position): string {
  return `${p.row},${p.col}`;
}

function stateKey(state: SearchState): string {
  const boxParts = state.boxes
    .map((b) => `${b.position.row},${b.position.col}`)
    .sort()
    .join("|");
  return `${state.robot.row},${state.robot.col}:${boxParts}`;
}

function offset(pos: Position, dir: Direction): Position {
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

function isSolved(board: BoardState, boxes: SearchState["boxes"]): boolean {
  return boxes.every((box) =>
    board.goals.some(
      (g) =>
        g.position.row === box.position.row &&
        g.position.col === box.position.col &&
        g.label === box.label,
    ),
  );
}

export interface OracleResult {
  solvable: boolean;
  moves: number;
  pushes: number;
  path: Direction[];
  statesExplored: number;
}

export function bfsSolve(
  rows: string[],
  maxStates: number = 500_000,
): OracleResult {
  const walls = new Set<string>();
  const goals: BoardState["goals"] = [];
  let robot: Position = { row: 0, col: 0 };
  const boxes: SearchState["boxes"] = [];

  for (let r = 0; r < rows.length; r++) {
    const row = rows[r]!;
    for (let c = 0; c < row.length; c++) {
      const ch = row[c]!;
      const pos: Position = { row: r, col: c };
      if (ch === "O") walls.add(cellKey(pos));
      else if (ch === "R") robot = pos;
      else if (ch === "X") boxes.push({ position: pos, label: "X" });
      else if (ch === "S") goals.push({ position: pos, label: "X" });
      else if (ch >= "A" && ch <= "Z" && !"ORSX".includes(ch))
        boxes.push({ position: pos, label: ch });
      else if (ch >= "a" && ch <= "z")
        goals.push({ position: pos, label: ch.toUpperCase() });
    }
  }

  const board: BoardState = { walls, goals };
  const initial: SearchState = { robot, boxes };

  if (isSolved(board, initial.boxes)) {
    return { solvable: true, moves: 0, pushes: 0, path: [], statesExplored: 0 };
  }

  const visited = new Set<string>();
  const queue: Array<{ state: SearchState; path: Direction[]; pushes: number }> =
    [];

  const initKey = stateKey(initial);
  visited.add(initKey);
  queue.push({ state: initial, path: [], pushes: 0 });

  let explored = 0;
  let head = 0;

  while (head < queue.length && explored < maxStates) {
    const current = queue[head++]!;
    explored++;

    for (const dir of DIRECTIONS) {
      const target = offset(current.state.robot, dir);

      if (walls.has(cellKey(target))) continue;

      const boxIdx = current.state.boxes.findIndex(
        (b) => b.position.row === target.row && b.position.col === target.col,
      );

      let nextState: SearchState;
      let nextPushes = current.pushes;

      if (boxIdx >= 0) {
        const pushTarget = offset(target, dir);
        if (walls.has(cellKey(pushTarget))) continue;
        if (
          current.state.boxes.some(
            (b) =>
              b.position.row === pushTarget.row &&
              b.position.col === pushTarget.col,
          )
        )
          continue;

        const newBoxes = current.state.boxes.map((b, i) =>
          i === boxIdx ? { ...b, position: pushTarget } : { ...b },
        );
        nextState = { robot: target, boxes: newBoxes };
        nextPushes++;
      } else {
        nextState = { robot: target, boxes: current.state.boxes };
      }

      const key = stateKey(nextState);
      if (visited.has(key)) continue;
      visited.add(key);

      const newPath = [...current.path, dir];

      if (isSolved(board, nextState.boxes)) {
        return {
          solvable: true,
          moves: newPath.length,
          pushes: nextPushes,
          path: newPath,
          statesExplored: explored,
        };
      }

      queue.push({ state: nextState, path: newPath, pushes: nextPushes });
    }
  }

  return {
    solvable: false,
    moves: 0,
    pushes: 0,
    path: [],
    statesExplored: explored,
  };
}
