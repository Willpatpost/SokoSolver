export const DIFFICULTIES = [
  "tutorial",
  "beginner",
  "intermediate",
  "advanced",
  "expert",
  "master",
] as const;

export type Difficulty = (typeof DIFFICULTIES)[number];

export interface PuzzleDefinition {
  readonly id: string;
  readonly title: string;
  readonly difficulty: Difficulty;
  readonly boxes: number;
  readonly hint?: string;
  readonly rows: readonly string[];
}

export const DIRECTIONS = ["up", "down", "left", "right"] as const;

export type Direction = (typeof DIRECTIONS)[number];

export interface Position {
  readonly row: number;
  readonly column: number;
}

export interface Box {
  readonly id: string;
  readonly label: string;
  readonly position: Position;
}

export interface Goal {
  readonly label: string;
  readonly position: Position;
}

export interface ParsedBoard {
  readonly width: number;
  readonly height: number;
  readonly rows: readonly string[];
  readonly walls: readonly Position[];
  readonly floor: readonly Position[];
  readonly goals: readonly Goal[];
  readonly initialRobot: Position;
  readonly initialBoxes: readonly Box[];
}

export interface GameSnapshot {
  readonly puzzleId: string;
  readonly robot: Position;
  readonly boxes: readonly Box[];
  readonly moves: number;
  readonly pushes: number;
  readonly solved: boolean;
}

export interface SnapshotTransition {
  readonly snapshot: GameSnapshot;
  readonly moved: boolean;
  readonly pushed: boolean;
}
