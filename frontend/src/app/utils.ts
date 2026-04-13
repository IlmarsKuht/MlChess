import { Chess } from "chess.js";

import type {
  BoardMoveMarker,
  GameResult,
  Participant,
  ReplayPayload,
  RouteState,
  TimeControl,
  TournamentKind,
  Variant,
  WorkspaceView
} from "./types";

export const liveRevealDelayMs = 600;
export const lastWatchedKey = "arena:last-watched-match";

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

export function formatVariant(value: Variant) {
  return value === "standard" ? "Standard" : "Chess960";
}

export function formatTournamentKind(value: TournamentKind) {
  return value === "round_robin" ? "Round robin" : "Ladder";
}

export function formatLabel(value: string) {
  return value
    .split("_")
    .map((token) => token.charAt(0).toUpperCase() + token.slice(1))
    .join(" ");
}

export function formatDuration(ms: number) {
  if (ms >= 1000) {
    const seconds = ms / 1000;
    return Number.isInteger(seconds) ? `${seconds}s` : `${seconds.toFixed(1)}s`;
  }
  return `${ms}ms`;
}

export function isTerminalLiveStatus(status: string) {
  return status === "completed" || status === "failed" || status === "skipped" || status === "finished" || status === "aborted";
}

export function formatTimeControl(timeControl: TimeControl) {
  return `${formatDuration(timeControl.initial_ms)} + ${formatDuration(timeControl.increment_ms)}`;
}

export function formatClock(ms: number) {
  if (!Number.isFinite(ms)) {
    return "--:--";
  }
  const totalSeconds = Math.max(0, Math.floor(ms / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

export function liveClockElapsedMs(options: {
  status?: string;
  isLiveFollowing: boolean;
  liveNowMs: number;
  turnStartedServerUnixMs?: number;
}) {
  const { status, isLiveFollowing, liveNowMs, turnStartedServerUnixMs } = options;
  if (!isLiveFollowing || status !== "running" || turnStartedServerUnixMs === undefined) {
    return 0;
  }
  return Math.max(0, liveNowMs - turnStartedServerUnixMs);
}

export function formatRelativeTime(timestamp: number) {
  const elapsedMs = Math.max(0, Date.now() - timestamp);
  const elapsedSeconds = Math.floor(elapsedMs / 1000);
  if (elapsedSeconds < 1) {
    return "just now";
  }
  if (elapsedSeconds === 1) {
    return "1s ago";
  }
  return `${elapsedSeconds}s ago`;
}

export function loadErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : "Request failed";
}

export function participantName(participant?: Participant | null, fallback = "Player") {
  return participant?.display_name ?? fallback;
}

export function squareName(index: number) {
  const file = "abcdefgh"[index % 8];
  const rank = String(8 - Math.floor(index / 8));
  return `${file}${rank}`;
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

export function boardIndexToSquare(index: number, orientation: "white" | "black") {
  return squareName(orientation === "white" ? index : 63 - index);
}

export function parseRoute(hash: string): RouteState {
  const normalized = hash.replace(/^#/, "");
  const [routePath = ""] = normalized.split("?");
  const parts = routePath.split("/").filter(Boolean);

  if (parts[0] === "watch" && parts[1]) {
    return { page: "watch", matchId: decodeURIComponent(parts[1]) };
  }

  if (parts[0] === "engine" && parts[1]) {
    return { page: "engine", engineId: decodeURIComponent(parts[1]) };
  }

  switch (parts[0]) {
    case "setup":
      return { page: "app", view: "setup" };
    case "live-duel":
      return { page: "app", view: "live_duel" };
    case "play-engine":
      return { page: "app", view: "play_engine" };
    case "events":
      return { page: "app", view: "events" };
    case "tournaments":
      return { page: "app", view: "tournament" };
    case "replay":
      return { page: "app", view: "replay" };
    default:
      return { page: "app", view: "overview" };
  }
}

export function viewHash(view: WorkspaceView) {
  switch (view) {
    case "setup":
      return "#/setup";
    case "live_duel":
      return "#/live-duel";
    case "play_engine":
      return "#/play-engine";
    case "events":
      return "#/events";
    case "tournament":
      return "#/tournaments";
    case "replay":
      return "#/replay";
    default:
      return "#/";
  }
}

export function watchHash(matchId: string) {
  return `#/watch/${encodeURIComponent(matchId)}`;
}

export function engineHash(engineId: string) {
  return `#/engine/${encodeURIComponent(engineId)}`;
}

export function navigateToHash(hash: string) {
  window.location.hash = hash;
}

export function statusTone(value: string) {
  switch (value) {
    case "running":
      return "running" as const;
    case "completed":
      return "good" as const;
    case "failed":
    case "stopped":
      return "warning" as const;
    default:
      return "quiet" as const;
  }
}

export function sideToMove(startFen: string, ply: number) {
  const turn = startFen.split(" ")[1] === "b" ? "black" : "white";
  if (ply % 2 === 0) {
    return turn;
  }
  return turn === "white" ? "black" : "white";
}

export function matchResultText(result?: GameResult | null) {
  switch (result) {
    case "white_win":
      return "1-0";
    case "black_win":
      return "0-1";
    case "draw":
      return "1/2-1/2";
    default:
      return "In progress";
  }
}

export function winnerText(result?: GameResult | null) {
  switch (result) {
    case "white_win":
      return "White won";
    case "black_win":
      return "Black won";
    case "draw":
      return "Draw";
    default:
      return "Running";
  }
}

export function roundLabel(kind: TournamentKind, roundIndex: number) {
  return kind === "ladder" ? `Step ${roundIndex + 1}` : `Round ${roundIndex + 1}`;
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

export const workspaceViews: Array<{ id: WorkspaceView; label: string; detail: string }> = [
  { id: "overview", label: "Home", detail: "Live standings and recent games" },
  { id: "setup", label: "Engines", detail: "Available engines and formats" },
  { id: "live_duel", label: "Live Duel", detail: "Start duels and see live matches" },
  { id: "play_engine", label: "Play vs Engine", detail: "Launch and play human games" },
  { id: "events", label: "Events", detail: "Start backend-defined runs" },
  { id: "tournament", label: "Tournaments", detail: "Bracket-like matchup map" },
  { id: "replay", label: "Replay", detail: "Boards, moves and results" }
];
