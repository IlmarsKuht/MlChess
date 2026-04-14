import { groupedMoveRows } from "../chess/board";
import { EmptyState } from "./EmptyState";

export function MoveList({ moves, activePly }: { moves: string[]; activePly: number }) {
  const rows = groupedMoveRows(moves);

  if (rows.length === 0) {
    return <EmptyState>No moves revealed yet.</EmptyState>;
  }

  return (
    <div className="move-list">
      {rows.map((row) => {
        const whitePly = (row.index - 1) * 2 + 1;
        const blackPly = whitePly + 1;

        return (
          <div className="move-row" key={row.index}>
            <span className="move-index">{row.index}.</span>
            <span className={activePly === whitePly ? "move-active" : ""}>{row.white}</span>
            <span className={activePly === blackPly ? "move-active" : ""}>{row.black ?? ""}</span>
          </div>
        );
      })}
    </div>
  );
}
