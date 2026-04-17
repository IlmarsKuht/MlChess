import type { BenchmarkPool, TimeControl, Variant } from "../api/types";

export function timeControlKey(timeControl: TimeControl) {
  return `${timeControl.initial_ms}:${timeControl.increment_ms}`;
}

export function sameTimeControl(left: TimeControl, right: TimeControl) {
  return left.initial_ms === right.initial_ms && left.increment_ms === right.increment_ms;
}

export function uniquePoolVariants(pools: BenchmarkPool[]) {
  const variants = new Set(pools.map((pool) => pool.variant));
  return (["standard", "chess960"] as Variant[]).filter((variant) => variants.has(variant));
}

export function uniquePoolTimeControls(pools: BenchmarkPool[]) {
  const seen = new Set<string>();
  return pools
    .map((pool) => pool.time_control)
    .filter((timeControl) => {
      const key = timeControlKey(timeControl);
      if (seen.has(key)) {
        return false;
      }
      seen.add(key);
      return true;
    })
    .sort((left, right) => {
      if (left.initial_ms !== right.initial_ms) {
        return left.initial_ms - right.initial_ms;
      }
      return left.increment_ms - right.increment_ms;
    });
}

export function findPoolForChoices(
  pools: BenchmarkPool[],
  variant: Variant,
  timeControlKeyValue: string
) {
  return (
    pools.find(
      (pool) => pool.variant === variant && timeControlKey(pool.time_control) === timeControlKeyValue
    ) ?? null
  );
}
