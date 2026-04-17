import { Chess } from "chess.js";

import type { BoardMoveMarker, ReplayPayload, Variant } from "../api/types";

export function buildFrames(variant: Variant, startFen: string, movesUci: string[]) {
  if (variant !== "standard") {
    return [];
  }

  try {
    const chess = new Chess(startFen);
    const frames = [chess.fen()];
    for (const move of movesUci) {
      chess.move({
        from: move.slice(0, 2),
        to: move.slice(2, 4),
        promotion: move.length > 4 ? (move[4] as "q" | "r" | "b" | "n") : undefined
      });
      frames.push(chess.fen());
    }
    return frames;
  } catch {
    return [];
  }
}

export function buildReplayFrames(replay: ReplayPayload | null) {
  if (!replay) {
    return [];
  }
  if (replay.frames.length > 0) {
    return replay.frames;
  }
  return buildFrames(replay.variant, replay.start_fen, replay.moves_uci);
}

export function fenToBoard(fen: string) {
  const [placement] = fen.split(" ");
  const squares: string[] = [];
  placement.split("/").forEach((rank) => {
    rank.split("").forEach((token) => {
      const count = Number(token);
      if (Number.isNaN(count)) {
        squares.push(token);
      } else {
        for (let index = 0; index < count; index += 1) {
          squares.push("");
        }
      }
    });
  });
  return squares;
}

export function orientSquares(squares: string[], orientation: "white" | "black") {
  return orientation === "white" ? squares : [...squares].reverse();
}

export function squareName(index: number) {
  const file = "abcdefgh"[index % 8];
  const rank = String(8 - Math.floor(index / 8));
  return `${file}${rank}`;
}

export function boardIndexToSquare(index: number, orientation: "white" | "black") {
  return squareName(orientation === "white" ? index : 63 - index);
}

export function maybePromotion(_from: string, to: string, piece: string) {
  if (piece.toLowerCase() !== "p") {
    return "";
  }
  const promotionRank = piece === "P" ? "8" : "1";
  return to.endsWith(promotionRank) ? "q" : "";
}

export function legalMovesByOrigin(fen: string) {
  try {
    const chess = new Chess(fen);
    const verboseMoves = chess.moves({ verbose: true });
    const movesByOrigin = new Map<string, BoardMoveMarker[]>();
    for (const move of verboseMoves) {
      const current = movesByOrigin.get(move.from) ?? [];
      current.push({
        square: move.to,
        kind: move.captured ? "capture" : "quiet"
      });
      movesByOrigin.set(move.from, current);
    }
    return movesByOrigin;
  } catch {
    return new Map<string, BoardMoveMarker[]>();
  }
}

export function groupedMoveRows(moves: string[]) {
  const rows: Array<{ index: number; white: string; black?: string }> = [];
  for (let index = 0; index < moves.length; index += 2) {
    rows.push({
      index: Math.floor(index / 2) + 1,
      white: moves[index],
      black: moves[index + 1]
    });
  }
  return rows;
}
