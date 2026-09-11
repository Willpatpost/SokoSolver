export default function init(): Promise<void>;
export function solver_version(): string;
export function solve(puzzle_json: string, options_json: string): string;
export function solve_with_progress(
  puzzle_json: string,
  options_json: string,
  on_progress: (progress_json: string) => boolean,
): string;
export function parse_worker_command(json: string): string;
export function protocol_version(): number;
