import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { bfsSolve } from "./bfs-oracle.ts";

describe("bfsSolve", () => {
  it("solves trivial 1-box puzzle", () => {
    const rows = ["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
    const result = bfsSolve(rows);
    assert.ok(result.solvable);
    assert.equal(result.pushes, 2);
    assert.ok(result.moves >= result.pushes);
    assert.ok(result.path.length === result.moves);
  });

  it("solves trivial 1-box inline", () => {
    const rows = ["OOOOO", "OSXRO", "OOOOO"];
    const result = bfsSolve(rows);
    assert.ok(result.solvable);
    assert.equal(result.pushes, 1);
    assert.equal(result.moves, 1);
    assert.deepEqual(result.path, ["Left"]);
  });

  it("solves two-box horizontal", () => {
    const rows = [
      "OOOOOOO",
      "O S S O",
      "O     O",
      "O X X O",
      "O  R  O",
      "OOOOOOO",
    ];
    const result = bfsSolve(rows);
    assert.ok(result.solvable);
    assert.equal(result.pushes, 4);
  });

  it("solves typed 2-box puzzle", () => {
    const rows = [
      "OOOOOOO",
      "O a   O",
      "O AR  O",
      "O B   O",
      "O   b O",
      "OOOOOOO",
    ];
    const result = bfsSolve(rows);
    assert.ok(result.solvable);
    assert.equal(result.pushes, 4);
  });

  it("detects unsolvable puzzle", () => {
    const rows = ["OOOOO", "OX  O", "O  SO", "O  RO", "OOOOO"];
    const result = bfsSolve(rows);
    assert.ok(!result.solvable);
  });

  it("no boxes or goals means trivially solved", () => {
    const rows = ["OOOOO", "O R O", "O   O", "OOOOO"];
    const result = bfsSolve(rows);
    assert.ok(result.solvable);
    assert.equal(result.moves, 0);
    assert.equal(result.pushes, 0);
  });

  it("returns statesExplored > 0 for non-trivial puzzles", () => {
    const rows = ["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
    const result = bfsSolve(rows);
    assert.ok(result.statesExplored > 0);
  });

  it("respects maxStates limit", () => {
    const rows = [
      "OOOOOOOOO",
      "OSSS    O",
      "O       O",
      "O XXX   O",
      "O   R   O",
      "O       O",
      "OOOOOOOOO",
    ];
    const result = bfsSolve(rows, 100);
    assert.ok(result.statesExplored <= 100);
  });

  it("path directions are valid", () => {
    const rows = ["OOOOO", "O  SO", "O XRO", "O   O", "OOOOO"];
    const result = bfsSolve(rows);
    assert.ok(result.solvable);
    const valid = new Set(["Up", "Down", "Left", "Right"]);
    for (const dir of result.path) {
      assert.ok(valid.has(dir), `invalid direction: ${dir}`);
    }
  });

  it("solves l-push puzzle", () => {
    const rows = ["OOOOO", "O  SO", "O X O", "O R O", "OOOOO"];
    const result = bfsSolve(rows);
    assert.ok(result.solvable);
    assert.equal(result.pushes, 2);
  });
});
