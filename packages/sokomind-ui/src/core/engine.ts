import type {
  Box,
  Direction,
  GameSnapshot,
  Goal,
  ParsedBoard,
  Position,
  PuzzleDefinition,
  SnapshotTransition,
} from "./types.ts";

const WALL = "O";

const DELTAS: Readonly<Record<Direction, Position>> = {
  up: { row: -1, column: 0 },
  down: { row: 1, column: 0 },
  left: { row: 0, column: -1 },
  right: { row: 0, column: 1 },
};

function numericKey(row: number, column: number, width: number): number {
  return row * width + column;
}

function isDedicatedBox(ch: string): boolean {
  return /^[A-Z]$/.test(ch) && !"ORSX".includes(ch);
}

function goalLabel(ch: string): string | undefined {
  if (ch === "S") return "X";
  if (/^[a-z]$/.test(ch)) return ch.toUpperCase();
  return undefined;
}

export function parsePuzzle(puzzle: PuzzleDefinition): ParsedBoard {
  const width = Math.max(...puzzle.rows.map((r) => r.length));
  const normalizedRows = puzzle.rows.map((r) => r.padEnd(width, WALL));

  const walls: Position[] = [];
  const floor: Position[] = [];
  const goals: Goal[] = [];
  const initialBoxes: Box[] = [];
  const boxIndexes = new Map<string, number>();
  let initialRobot: Position = { row: 0, column: 0 };

  for (let r = 0; r < normalizedRows.length; r++) {
    const row = normalizedRows[r]!;
    for (let c = 0; c < row.length; c++) {
      const ch = row[c]!;
      const pos: Position = { row: r, column: c };

      if (ch === WALL) {
        walls.push(pos);
        continue;
      }

      floor.push(pos);

      if (ch === "R") {
        initialRobot = pos;
      } else if (ch === "X" || isDedicatedBox(ch)) {
        const idx = boxIndexes.get(ch) ?? 0;
        boxIndexes.set(ch, idx + 1);
        initialBoxes.push({ id: `${ch}:${idx}`, label: ch, position: pos });
      }

      const gl = goalLabel(ch);
      if (gl) goals.push({ label: gl, position: pos });
    }
  }

  return Object.freeze({
    width,
    height: normalizedRows.length,
    rows: Object.freeze(normalizedRows),
    walls: Object.freeze(walls),
    floor: Object.freeze(floor),
    goals: Object.freeze(goals),
    initialRobot,
    initialBoxes: Object.freeze(initialBoxes),
  });
}

function boxesAreSolved(board: ParsedBoard, boxes: readonly Box[]): boolean {
  const goalMap = new Map<number, string>();
  for (const g of board.goals) {
    goalMap.set(numericKey(g.position.row, g.position.column, board.width), g.label);
  }
  return boxes.every(
    (b) =>
      goalMap.get(numericKey(b.position.row, b.position.column, board.width)) ===
      b.label,
  );
}

export function createSnapshot(
  puzzleId: string,
  board: ParsedBoard,
  robot: Position,
  boxes: readonly Box[],
  moves: number,
  pushes: number,
): GameSnapshot {
  return Object.freeze({
    puzzleId,
    robot,
    boxes,
    moves,
    pushes,
    solved: boxesAreSolved(board, boxes),
  });
}

function isFloor(board: ParsedBoard, pos: Position): boolean {
  if (pos.row < 0 || pos.column < 0 || pos.row >= board.height || pos.column >= board.width) {
    return false;
  }
  return board.rows[pos.row]?.[pos.column] !== WALL;
}

export function stepSnapshot(
  board: ParsedBoard,
  snapshot: GameSnapshot,
  direction: Direction,
): SnapshotTransition {
  const delta = DELTAS[direction];
  const dest: Position = {
    row: snapshot.robot.row + delta.row,
    column: snapshot.robot.column + delta.column,
  };

  if (!isFloor(board, dest)) {
    return { snapshot, moved: false, pushed: false };
  }

  const boxMap = new Map<number, number>();
  for (let i = 0; i < snapshot.boxes.length; i++) {
    const b = snapshot.boxes[i]!;
    boxMap.set(numericKey(b.position.row, b.position.column, board.width), i);
  }

  const destKey = numericKey(dest.row, dest.column, board.width);
  const pushedIdx = boxMap.get(destKey);

  let boxes = snapshot.boxes;
  let pushes = snapshot.pushes;
  let pushed = false;

  if (pushedIdx !== undefined) {
    const boxDest: Position = {
      row: dest.row + delta.row,
      column: dest.column + delta.column,
    };
    const boxDestKey = numericKey(boxDest.row, boxDest.column, board.width);

    if (!isFloor(board, boxDest) || boxMap.has(boxDestKey)) {
      return { snapshot, moved: false, pushed: false };
    }

    boxes = Object.freeze(
      snapshot.boxes.map((b, i) =>
        i === pushedIdx ? Object.freeze({ ...b, position: boxDest }) : b,
      ),
    );
    pushes += 1;
    pushed = true;
  }

  const next = createSnapshot(snapshot.puzzleId, board, dest, boxes, snapshot.moves + 1, pushes);
  return { snapshot: next, moved: true, pushed };
}

export function isSolved(snapshot: GameSnapshot): boolean {
  return snapshot.solved;
}
