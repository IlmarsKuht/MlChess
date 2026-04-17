import { describe, expect, it } from "vitest";

import type { BenchmarkPool } from "../api/types";
import { findPoolForChoices, timeControlKey, uniquePoolTimeControls, uniquePoolVariants } from "./pools";

const pools: BenchmarkPool[] = [
  {
    id: "standard-1-0",
    name: "Bullet 1+0",
    variant: "standard",
    time_control: { initial_ms: 60_000, increment_ms: 0 },
    fairness: { paired_games: true, swap_colors: true }
  },
  {
    id: "chess960-1-0",
    name: "Chess960 Bullet 1+0",
    variant: "chess960",
    time_control: { initial_ms: 60_000, increment_ms: 0 },
    fairness: { paired_games: true, swap_colors: true }
  },
  {
    id: "standard-2-1",
    name: "Bullet 2+1",
    variant: "standard",
    time_control: { initial_ms: 120_000, increment_ms: 1_000 },
    fairness: { paired_games: true, swap_colors: true }
  }
];

describe("pool choice helpers", () => {
  it("deduplicates variants and time controls from registered pools", () => {
    expect(uniquePoolVariants(pools)).toEqual(["standard", "chess960"]);
    expect(uniquePoolTimeControls(pools)).toEqual([
      { initial_ms: 60_000, increment_ms: 0 },
      { initial_ms: 120_000, increment_ms: 1_000 }
    ]);
  });

  it("resolves a separate variant and time-control choice back to a pool", () => {
    expect(findPoolForChoices(pools, "chess960", timeControlKey({ initial_ms: 60_000, increment_ms: 0 }))?.id).toBe(
      "chess960-1-0"
    );
    expect(findPoolForChoices(pools, "chess960", timeControlKey({ initial_ms: 120_000, increment_ms: 1_000 }))).toBeNull();
  });
});
