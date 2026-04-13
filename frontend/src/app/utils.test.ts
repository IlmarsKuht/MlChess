import { describe, expect, it } from "vitest";

import type { MatchSeries } from "./types";
import { isPendingLiveWatchMatch, liveClockElapsedMs, pendingLiveWatchWindowMs } from "./utils";

describe("liveClockElapsedMs", () => {
  it("keeps the clock frozen when the viewer is not following live", () => {
    expect(
      liveClockElapsedMs({
        status: "running",
        isLiveFollowing: false,
        liveNowMs: 5_000,
        turnStartedServerUnixMs: 1_000
      })
    ).toBe(0);
  });

  it("counts down only for the live running frame", () => {
    expect(
      liveClockElapsedMs({
        status: "running",
        isLiveFollowing: true,
        liveNowMs: 5_000,
        turnStartedServerUnixMs: 1_000
      })
    ).toBe(4_000);
  });
});

describe("isPendingLiveWatchMatch", () => {
  const baseMatch: MatchSeries = {
    id: "match-id",
    tournament_id: "tournament-id",
    pool_id: "pool-id",
    round_index: 0,
    white_version_id: "white-id",
    black_version_id: "black-id",
    opening_id: null,
    game_index: 0,
    status: "running",
    watch_state: "unavailable",
    game_id: null,
    created_at: new Date(1_000).toISOString(),
    white_participant: { kind: "engine_version", id: "white-id", display_name: "White" },
    black_participant: { kind: "engine_version", id: "black-id", display_name: "Black" },
    interactive: false
  };

  it("treats a newly created running unavailable match as pending live startup", () => {
    expect(isPendingLiveWatchMatch(baseMatch, 1_000 + pendingLiveWatchWindowMs - 1)).toBe(true);
  });

  it("stops treating stale running unavailable matches as pending", () => {
    expect(isPendingLiveWatchMatch(baseMatch, 1_000 + pendingLiveWatchWindowMs + 1)).toBe(false);
  });
});
