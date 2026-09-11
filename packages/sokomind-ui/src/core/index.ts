export type {
  Box,
  Difficulty,
  Direction,
  GameSnapshot,
  Goal,
  ParsedBoard,
  Position,
  PuzzleDefinition,
  SnapshotTransition,
} from "./types.ts";
export { DIFFICULTIES, DIRECTIONS } from "./types.ts";
export {
  createSnapshot,
  isSolved,
  parsePuzzle,
  stepSnapshot,
} from "./engine.ts";
