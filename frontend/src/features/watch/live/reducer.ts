import type { LiveMatchSnapshot, LiveProtocolEvent } from "../../../shared/api/types";
import {
  type ConfirmedLiveState,
  frameFromSnapshot,
  snapshotFromEvent
} from "./protocol";

export type ReduceResult =
  | { kind: "noop"; state: ConfirmedLiveState | null }
  | { kind: "next"; state: ConfirmedLiveState }
  | { kind: "gap" };

export function isTerminalSnapshot(snapshot: LiveMatchSnapshot | null | undefined) {
  return snapshot?.status === "finished" || snapshot?.status === "aborted";
}

export function reduceEvent(current: ConfirmedLiveState | null, event: LiveProtocolEvent): ReduceResult {
  if (event.event_type === "snapshot") {
    if (current?.snapshot && event.seq <= current.snapshot.seq) {
      return { kind: "noop", state: current };
    }
    return {
      kind: "next",
      state: {
        snapshot: event,
        timeline: [frameFromSnapshot(event)]
      }
    };
  }

  if (!current?.snapshot) {
    return { kind: "gap" };
  }
  if (event.seq < current.snapshot.seq) {
    return { kind: "noop", state: current };
  }
  if (event.seq === current.snapshot.seq) {
    return { kind: "noop", state: current };
  }
  if (event.seq > current.snapshot.seq + 1) {
    return { kind: "gap" };
  }

  const nextSnapshot = snapshotFromEvent(current.snapshot, event);
  const nextFrame = frameFromSnapshot(nextSnapshot);
  if (event.event_type === "clock_sync") {
    const lastFrame = current.timeline.at(-1);
    const timeline =
      lastFrame && lastFrame.moves.length === nextFrame.moves.length
        ? [...current.timeline.slice(0, -1), nextFrame]
        : [...current.timeline, nextFrame];
    return { kind: "next", state: { snapshot: nextSnapshot, timeline } };
  }

  return {
    kind: "next",
    state: {
      snapshot: nextSnapshot,
      timeline: [...current.timeline, nextFrame]
    }
  };
}
