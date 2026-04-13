import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { useLivePlayback } from "./livePlayback";
import { liveRevealDelayMs } from "./utils";

describe("useLivePlayback", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("delays only new frames that arrive while the user is actively watching", () => {
    vi.useFakeTimers();

    const { result, rerender } = renderHook(
      ({ activelyWatching, interactive, liveFrameCount, matchId }) =>
        useLivePlayback({ activelyWatching, interactive, liveFrameCount, matchId }),
      {
        initialProps: {
          activelyWatching: true,
          interactive: false,
          liveFrameCount: 3,
          matchId: "match-1"
        }
      }
    );

    expect(result.current.displayedLiveFrameCount).toBe(3);
    expect(result.current.selectedLivePly).toBe(2);
    expect(result.current.isLiveFollowing).toBe(true);

    rerender({
      activelyWatching: true,
      interactive: false,
      liveFrameCount: 5,
      matchId: "match-1"
    });

    expect(result.current.displayedLiveFrameCount).toBe(3);
    expect(result.current.selectedLivePly).toBe(2);

    act(() => {
      vi.advanceTimersByTime(liveRevealDelayMs);
    });
    expect(result.current.displayedLiveFrameCount).toBe(4);
    expect(result.current.selectedLivePly).toBe(3);

    act(() => {
      vi.advanceTimersByTime(liveRevealDelayMs);
    });
    expect(result.current.displayedLiveFrameCount).toBe(5);
    expect(result.current.selectedLivePly).toBe(4);
  });

  it("catches up immediately and re-enables follow when leaving the watch route", () => {
    const { result, rerender } = renderHook(
      ({ activelyWatching, interactive, liveFrameCount, matchId }) =>
        useLivePlayback({ activelyWatching, interactive, liveFrameCount, matchId }),
      {
        initialProps: {
          activelyWatching: true,
          interactive: false,
          liveFrameCount: 3,
          matchId: "match-1"
        }
      }
    );

    act(() => {
      result.current.setSelectedLivePly(0);
    });
    expect(result.current.isLiveFollowing).toBe(false);
    expect(result.current.selectedLivePly).toBe(0);

    rerender({
      activelyWatching: false,
      interactive: false,
      liveFrameCount: 5,
      matchId: "match-1"
    });

    expect(result.current.displayedLiveFrameCount).toBe(5);
    expect(result.current.selectedLivePly).toBe(4);
    expect(result.current.isLiveFollowing).toBe(true);
  });

  it("stays caught up off-watch and resumes delayed reveal only for future frames after reopening", () => {
    vi.useFakeTimers();

    const { result, rerender } = renderHook(
      ({ activelyWatching, interactive, liveFrameCount, matchId }) =>
        useLivePlayback({ activelyWatching, interactive, liveFrameCount, matchId }),
      {
        initialProps: {
          activelyWatching: true,
          interactive: false,
          liveFrameCount: 2,
          matchId: "match-1"
        }
      }
    );

    rerender({
      activelyWatching: false,
      interactive: false,
      liveFrameCount: 4,
      matchId: "match-1"
    });

    expect(result.current.displayedLiveFrameCount).toBe(4);
    expect(result.current.selectedLivePly).toBe(3);

    act(() => {
      vi.advanceTimersByTime(liveRevealDelayMs * 3);
    });
    expect(result.current.displayedLiveFrameCount).toBe(4);

    rerender({
      activelyWatching: true,
      interactive: false,
      liveFrameCount: 4,
      matchId: "match-1"
    });

    expect(result.current.displayedLiveFrameCount).toBe(4);
    expect(result.current.selectedLivePly).toBe(3);

    rerender({
      activelyWatching: true,
      interactive: false,
      liveFrameCount: 5,
      matchId: "match-1"
    });

    expect(result.current.displayedLiveFrameCount).toBe(4);

    act(() => {
      vi.advanceTimersByTime(liveRevealDelayMs);
    });
    expect(result.current.displayedLiveFrameCount).toBe(5);
    expect(result.current.selectedLivePly).toBe(4);
  });

  it("keeps interactive games at full speed", () => {
    vi.useFakeTimers();

    const { result, rerender } = renderHook(
      ({ activelyWatching, interactive, liveFrameCount, matchId }) =>
        useLivePlayback({ activelyWatching, interactive, liveFrameCount, matchId }),
      {
        initialProps: {
          activelyWatching: true,
          interactive: true,
          liveFrameCount: 2,
          matchId: "match-1"
        }
      }
    );

    rerender({
      activelyWatching: true,
      interactive: true,
      liveFrameCount: 5,
      matchId: "match-1"
    });

    expect(result.current.displayedLiveFrameCount).toBe(5);
    expect(result.current.selectedLivePly).toBe(4);

    act(() => {
      vi.advanceTimersByTime(liveRevealDelayMs * 2);
    });
    expect(result.current.displayedLiveFrameCount).toBe(5);
  });
});
