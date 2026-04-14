import type { MatchSeries } from "../../app/types";

export const liveRevealDelayMs = 600;
export const lastWatchedKey = "arena:last-watched-match";
export const pendingLiveWatchWindowMs = 15_000;

export function isTerminalLiveStatus(status: string) {
  return status === "completed" || status === "failed" || status === "skipped" || status === "finished" || status === "aborted";
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

export function isPendingLiveWatchMatch(match: MatchSeries, nowMs = Date.now()) {
  if (match.status !== "running" || match.watch_state !== "unavailable") {
    return false;
  }

  const createdAtMs = new Date(match.created_at).getTime();
  if (!Number.isFinite(createdAtMs)) {
    return false;
  }

  return Math.max(0, nowMs - createdAtMs) <= pendingLiveWatchWindowMs;
}
