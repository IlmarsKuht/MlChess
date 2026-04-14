import { useEffect, useReducer } from "react";

import { liveRevealDelayMs } from "./model";

interface LivePlaybackContext {
  activelyWatching: boolean;
  interactive: boolean;
  liveFrameCount: number;
  matchId: string;
}

interface LivePlaybackState {
  displayedLiveFrameCount: number;
  isLiveFollowing: boolean;
  matchId: string;
  selectedLivePly: number;
}

type LivePlaybackAction =
  | ({ type: "sync_context" } & LivePlaybackContext)
  | { type: "reveal_next"; liveFrameCount: number }
  | { type: "set_following"; value: boolean }
  | { type: "set_selected_live_ply"; value: number };

export interface UseLivePlaybackOptions extends LivePlaybackContext {}

export interface LivePlaybackViewModel extends LivePlaybackState {
  returnToLive: () => void;
  setSelectedLivePly: (value: number) => void;
}

function latestPly(liveFrameCount: number) {
  return Math.max(liveFrameCount - 1, 0);
}

function fullSyncState(matchId: string, liveFrameCount: number): LivePlaybackState {
  return {
    matchId,
    displayedLiveFrameCount: liveFrameCount,
    selectedLivePly: latestPly(liveFrameCount),
    isLiveFollowing: true
  };
}

export function createInitialLivePlaybackState(): LivePlaybackState {
  return fullSyncState("", 0);
}

export function reduceLivePlaybackState(
  state: LivePlaybackState,
  action: LivePlaybackAction
): LivePlaybackState {
  switch (action.type) {
    case "sync_context": {
      const { activelyWatching, interactive, liveFrameCount, matchId } = action;
      if (!matchId) {
        return createInitialLivePlaybackState();
      }

      if (interactive || !activelyWatching || state.matchId !== matchId) {
        return fullSyncState(matchId, liveFrameCount);
      }

      const nextDisplayedLiveFrameCount = Math.min(state.displayedLiveFrameCount, liveFrameCount);
      const maxVisiblePly = latestPly(nextDisplayedLiveFrameCount);
      return {
        matchId,
        displayedLiveFrameCount: nextDisplayedLiveFrameCount,
        selectedLivePly: state.isLiveFollowing ? maxVisiblePly : Math.min(state.selectedLivePly, maxVisiblePly),
        isLiveFollowing: state.isLiveFollowing
      };
    }
    case "reveal_next": {
      const nextDisplayedLiveFrameCount = Math.min(state.displayedLiveFrameCount + 1, action.liveFrameCount);
      const nextVisiblePly = latestPly(nextDisplayedLiveFrameCount);
      return {
        ...state,
        displayedLiveFrameCount: nextDisplayedLiveFrameCount,
        selectedLivePly: state.isLiveFollowing ? nextVisiblePly : Math.min(state.selectedLivePly, nextVisiblePly)
      };
    }
    case "set_following":
      return {
        ...state,
        isLiveFollowing: action.value,
        selectedLivePly: action.value ? latestPly(state.displayedLiveFrameCount) : state.selectedLivePly
      };
    case "set_selected_live_ply": {
      const maxVisiblePly = latestPly(state.displayedLiveFrameCount);
      const nextSelectedLivePly = Math.min(action.value, maxVisiblePly);
      return {
        ...state,
        selectedLivePly: nextSelectedLivePly,
        isLiveFollowing: nextSelectedLivePly >= maxVisiblePly
      };
    }
  }
}

export function useLivePlayback(options: UseLivePlaybackOptions): LivePlaybackViewModel {
  const [state, dispatch] = useReducer(reduceLivePlaybackState, undefined, createInitialLivePlaybackState);

  useEffect(() => {
    dispatch({ type: "sync_context", ...options });
  }, [options.activelyWatching, options.interactive, options.liveFrameCount, options.matchId]);

  useEffect(() => {
    if (!options.matchId || options.interactive || !options.activelyWatching) {
      return;
    }

    if (state.displayedLiveFrameCount >= options.liveFrameCount) {
      return;
    }

    const timer = window.setTimeout(() => {
      dispatch({ type: "reveal_next", liveFrameCount: options.liveFrameCount });
    }, liveRevealDelayMs);

    return () => window.clearTimeout(timer);
  }, [
    options.activelyWatching,
    options.interactive,
    options.liveFrameCount,
    options.matchId,
    state.displayedLiveFrameCount
  ]);

  return {
    ...state,
    setSelectedLivePly: (value) => dispatch({ type: "set_selected_live_ply", value }),
    returnToLive: () => dispatch({ type: "set_following", value: true })
  };
}
