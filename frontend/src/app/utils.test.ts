import { describe, expect, it } from "vitest";

import { liveClockElapsedMs } from "./utils";

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
