import { pieceImages } from "../../app/pieces";
import type { BoardMoveMarker } from "../../app/types";
import { boardIndexToSquare } from "../chess/board";

export function BoardView({
  squares,
  selectedSquare,
  legalMoveMarkers = [],
  invalidSquare,
  interactive = false,
  hoverableSquares,
  onSquareClick,
  orientation = "white"
}: {
  squares: string[];
  selectedSquare?: string;
  legalMoveMarkers?: BoardMoveMarker[];
  invalidSquare?: string;
  interactive?: boolean;
  hoverableSquares?: Set<string>;
  onSquareClick?: (index: number) => void;
  orientation?: "white" | "black";
}) {
  const markerBySquare = new Map(legalMoveMarkers.map((marker) => [marker.square, marker.kind]));
  return (
    <div className="board-frame">
      <div className="board">
        {squares.map((piece, index) => {
          const square = boardIndexToSquare(index, orientation);
          const markerKind = markerBySquare.get(square);
          const hoverable = hoverableSquares?.has(square) ?? false;
          return (
            <button
              key={`${piece}-${index}`}
              type="button"
              className={`square ${(Math.floor(index / 8) + index) % 2 === 0 ? "light" : "dark"} ${
                selectedSquare === square ? "square-selected" : ""
              } ${invalidSquare === square ? "square-invalid" : ""} ${interactive ? "square-interactive" : ""} ${
                piece ? "square-has-piece" : ""
              } ${hoverable ? "square-hoverable-piece" : ""}`}
              onClick={() => onSquareClick?.(index)}
              disabled={!interactive}
            >
              {markerKind ? (
                <span
                  className={`square-marker ${
                    markerKind === "capture" ? "square-marker-capture" : "square-marker-quiet"
                  }`}
                  aria-hidden="true"
                />
              ) : null}
              {piece ? <img className="piece-image" src={pieceImages[piece]} alt="" draggable={false} /> : null}
            </button>
          );
        })}
      </div>
    </div>
  );
}
