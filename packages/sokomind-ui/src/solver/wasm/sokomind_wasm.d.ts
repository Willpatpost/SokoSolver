/* tslint:disable */
/* eslint-disable */

/**
 * Parse a worker command from JSON.
 *
 * Returns JSON-serialized result: either the parsed command
 * or an error envelope. Used by the worker to decode incoming messages.
 */
export function parse_worker_command(json: string): string;

/**
 * Get the worker protocol version.
 */
export function protocol_version(): number;

export function solve(puzzle_json: string, options_json: string): string;

/**
 * Solve a puzzle with periodic progress callbacks.
 *
 * `on_progress` receives a JSON string with phase, elapsed time,
 * and search counters. Return `true` to continue or `false` to cancel.
 */
export function solve_with_progress(puzzle_json: string, options_json: string, on_progress: Function): string;

export function solver_version(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly parse_worker_command: (a: number, b: number, c: number) => void;
    readonly protocol_version: () => number;
    readonly solve: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly solve_with_progress: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly solver_version: (a: number) => void;
    readonly __wbindgen_export: (a: number) => void;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export2: (a: number, b: number) => number;
    readonly __wbindgen_export3: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_export4: (a: number, b: number, c: number) => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
