# SokoSolver

A high-performance Sokoban puzzle solver built in Rust, compiled to WebAssembly
for browser deployment. Features beam search discovery, exact A*/IDA* proofs,
solution improvement, and comprehensive structured logging.

## Architecture

```
sokomind-core     Pure game rules: board parsing, transitions, replay
       |
sokomind-solver   Flagship solver: beam search, exact proof, improvement
       |
sokomind-wasm     WebAssembly bridge for browser execution
       |
sokomind-ui       React + TypeScript frontend (packages/sokomind-ui)
```

## Quick start

```bash
# Run solver tests
cargo test --workspace

# Build WASM module
wasm-pack build crates/sokomind-wasm --target web \
  --out-dir ../../packages/sokomind-ui/src/solver/wasm

# Start dev server (after WASM build)
cd packages/sokomind-ui && npm install && npm run dev
```

## Solver pipeline

1. **Preparation** — compile board, build reverse-push tables, analyze topology
2. **Structural plan** — doorway scheduling for large puzzles (10+ boxes)
3. **Beam discovery** — layered beam search with Pareto transposition
4. **Harvest + improve** — collect diverse solutions, window-based rewriting
5. **Proof** — exact A*/IDA* for move-optimal proof (quality/optimal mode)
6. **Verification** — replay solution through the core game engine

## License

MIT
