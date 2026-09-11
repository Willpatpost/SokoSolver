import { memo, type CSSProperties } from "react";
import type { GameSnapshot, Goal, ParsedBoard, Position } from "../../core/types.ts";
import styles from "./Board.module.css";

interface BoardProps {
  board: ParsedBoard;
  snapshot: GameSnapshot;
}

type BoardStyle = CSSProperties & {
  "--columns": number;
  "--rows": number;
};

type PieceStyle = CSSProperties & {
  "--piece-hue"?: number;
};

function typedHue(label: string): number {
  if (label === "X") return 32;
  return 14 + ((label.charCodeAt(0) - 65) * 47) % 300;
}

function posKey(p: Position): string {
  return `${p.row},${p.column}`;
}

interface StaticCellProps {
  isWall: boolean;
  goal: Goal | undefined;
}

const StaticCell = memo(function StaticCell({ isWall, goal }: StaticCellProps) {
  return (
    <div
      className={`${styles.cell} ${isWall ? styles.wall : styles.floor}`}
      aria-hidden="true"
    >
      {!isWall && goal ? (
        <span
          className={styles.goal}
          data-generic={goal.label === "X" || undefined}
          data-goal-label={goal.label}
          style={{ "--piece-hue": typedHue(goal.label) } as PieceStyle}
          aria-hidden="true"
        >
          <span>{goal.label === "X" ? "" : goal.label}</span>
        </span>
      ) : null}
    </div>
  );
});

interface BoxPieceProps {
  label: string;
  onGoal: boolean;
}

const BoxPiece = memo(function BoxPiece({ label, onGoal }: BoxPieceProps) {
  return (
    <span
      className={styles.box}
      data-generic={label === "X" || undefined}
      data-box-label={label}
      data-home={onGoal || undefined}
      style={{ "--piece-hue": typedHue(label) } as PieceStyle}
    >
      <span className={styles.crateFace}>
        <span className={styles.sigil}>
          {label === "X" ? "" : label}
        </span>
      </span>
    </span>
  );
});

const KeeperPiece = memo(function KeeperPiece() {
  return (
    <span className={styles.robot}>
      <span className={styles.antenna} />
      <span className={styles.robotFace} />
    </span>
  );
});

export function Board({ board, snapshot }: BoardProps) {
  const goalMap = new Map<string, Goal>();
  for (const g of board.goals) {
    goalMap.set(posKey(g.position), g);
  }

  const goalLabelAt = new Map<string, string>();
  for (const g of board.goals) {
    goalLabelAt.set(posKey(g.position), g.label);
  }

  const boardSize =
    board.width >= 12 || board.height >= 12 ? "large" : undefined;

  const boardStyle: BoardStyle = {
    "--columns": board.width,
    "--rows": board.height,
  };

  return (
    <div
      className={styles.board}
      style={boardStyle}
      data-solved={snapshot.solved || undefined}
      data-board-size={boardSize}
      role="grid"
      aria-label={`Sokoban puzzle, ${board.width} columns by ${board.height} rows`}
    >
      {board.rows.map((row, r) =>
        [...row].map((ch, c) => {
          const key = posKey({ row: r, column: c });
          return (
            <StaticCell
              key={key}
              isWall={ch === "O"}
              goal={goalMap.get(key)}
            />
          );
        }),
      )}

      <div className={styles.pieceLayer}>
        {Array.from({ length: board.height * board.width }, (_, i) => {
          const r = Math.floor(i / board.width);
          const c = i % board.width;
          const key = posKey({ row: r, column: c });

          if (snapshot.robot.row === r && snapshot.robot.column === c) {
            return (
              <span
                key="keeper"
                className={styles.pieceSlot}
                style={{ gridColumn: c + 1, gridRow: r + 1 }}
              >
                <KeeperPiece />
              </span>
            );
          }

          const box = snapshot.boxes.find(
            (b) => b.position.row === r && b.position.column === c,
          );
          if (box) {
            const onGoal = goalLabelAt.get(key) === box.label;
            return (
              <span
                key={box.id}
                className={styles.pieceSlot}
                style={{ gridColumn: c + 1, gridRow: r + 1 }}
              >
                <BoxPiece label={box.label} onGoal={onGoal} />
              </span>
            );
          }

          return null;
        })}
      </div>
    </div>
  );
}
