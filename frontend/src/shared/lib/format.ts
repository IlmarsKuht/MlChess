import type { GameResult, GameTermination, TimeControl, TournamentKind, Variant } from "../../app/types";

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

export function outcomeHeadline(result?: GameResult | null) {
  switch (result) {
    case "white_win":
      return "White wins";
    case "black_win":
      return "Black wins";
    case "draw":
      return "Draw";
    default:
      return "Game in progress";
  }
}

export function outcomeSubtitle(result?: GameResult | null, termination?: GameTermination | null) {
  if (!result) {
    return "The board is still live.";
  }
  const terminationLabel =
    termination && termination !== "none" ? ` by ${formatLabel(termination)}` : "";
  switch (result) {
    case "white_win":
      return `White takes the point${terminationLabel}.`;
    case "black_win":
      return `Black takes the point${terminationLabel}.`;
    case "draw":
      return `The game is shared${terminationLabel}.`;
    default:
      return "The board is still live.";
  }
}

export function roundLabel(kind: TournamentKind, roundIndex: number) {
  return kind === "ladder" ? `Step ${roundIndex + 1}` : `Round ${roundIndex + 1}`;
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
