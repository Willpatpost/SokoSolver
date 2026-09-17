# SokoSolver Engineering Rules

## Project overview

Rust + WebAssembly Sokoban solver with a React TypeScript frontend.
Three Rust crates: `sokomind-core` (game rules), `sokomind-solver` (algorithms),
`sokomind-wasm` (browser bridge). Deployed as a static site on GitHub Pages.

## Game rules

- O is wall.
- R is robot.
- X may only occupy S. Only X may occupy S.
- Typed uppercase boxes (A-Z, excluding O/R/S/X) occupy matching lowercase goals.
- Repeated typed labels are allowed.
- The robot pushes but never pulls.

## Solver truth rules

- The solver is move-optimal: g-cost = total moves (walks + pushes).
- Discovery (beam search) is bounded, not optimal.
- Only a completed exact move proof may set optimality to proven.
- A timeout is never proof.
- Every returned solution must replay through the core game engine.
- Exact proof uses collision-free state identity with exact keeper cell position.
- Beam search uses keeper zone (canonical minimum cell) for push-oriented transposition.
- Proof heuristics must be admissible (push-distance assignment is a lower bound on moves).
- Ordering heuristics may not affect proof f-cost.
- Incomplete local analysis is not a deadlock.

## Build commands

```bash
# Rust tests
cargo test --workspace
cargo test -p sokomind-core
cargo test -p sokomind-solver
cargo clippy --workspace -- -D warnings

# WASM build
wasm-pack build crates/sokomind-wasm --target web --out-dir ../../packages/sokomind-ui/src/solver/wasm

# TypeScript (when UI exists)
cd packages/sokomind-ui && npm run dev
cd packages/sokomind-ui && npm run build
cd packages/sokomind-ui && npm test
```

## Change workflow

- Keep each change focused and reversible.
- Add tests for changed behavior.
- Run `cargo test` and `cargo clippy` before committing.
