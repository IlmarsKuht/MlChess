import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";

import { WatchPage } from "./route";
import type { GameRecord, LiveMatchSnapshot, MatchSeries, Tournament } from "../../app/types";

const mocks = vi.hoisted(() => ({
  useMatchesQueryMock: vi.fn(),
  useGamesQueryMock: vi.fn(),
  usePoolsQueryMock: vi.fn(),
  useTournamentsQueryMock: vi.fn(),
  useConfirmedLiveMatchMock: vi.fn(),
  useLivePlaybackMock: vi.fn(),
  useReplayQueryMock: vi.fn(),
  setUiDebugStateMock: vi.fn()
}));

vi.mock("../../shared/queries/arena", () => ({
  useMatchesQuery: mocks.useMatchesQueryMock,
  useGamesQuery: mocks.useGamesQueryMock,
  usePoolsQuery: mocks.usePoolsQueryMock,
  useTournamentsQuery: mocks.useTournamentsQueryMock
}));

vi.mock("./live", () => ({
  useConfirmedLiveMatch: mocks.useConfirmedLiveMatchMock
}));

vi.mock("./livePlayback", () => ({
  useLivePlayback: mocks.useLivePlaybackMock
}));

vi.mock("../replay/api", () => ({
  useReplayQuery: mocks.useReplayQueryMock
}));

vi.mock("../../app/debug", () => ({
  setUiDebugState: mocks.setUiDebugStateMock
}));

vi.mock("../debug/DebugDrawer", () => ({
  DebugDrawer: () => null
}));

const match: MatchSeries = {
  id: "5ea5fbe8-bcec-4a3e-9ad2-65585c7824d2",
  tournament_id: "51cb0e9b-f196-487d-ac27-61b2800bd1b6",
  pool_id: "pool-1",
  round_index: 0,
  white_version_id: "white-version",
  black_version_id: "black-version",
  game_index: 0,
  status: "completed",
  watch_state: "replay",
  game_id: "game-1",
  created_at: "2026-04-14T18:47:00.000Z",
  white_participant: {
    kind: "engine_version",
    id: "white-version",
    display_name: "Engine White"
  },
  black_participant: {
    kind: "human_player",
    id: "human-player",
    display_name: "You"
  },
  interactive: true
};

const finishedSnapshot: LiveMatchSnapshot = {
  match_id: match.id,
  protocol_version: 1,
  event_type: "snapshot",
  seq: 58,
  server_now_unix_ms: 1776192502055,
  status: "finished",
  result: "white_win",
  termination: "timeout",
  start_fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
  fen: "2kr1b1r/1ppbq1pp/p1n2p2/4p3/3PB3/5N2/PPP1QPPP/R1B1R1K1 b - d3 0 12",
  moves: [
    "g1f3",
    "d7d5",
    "e2e3",
    "b8c6",
    "f1b5",
    "c8d7",
    "b1c3",
    "g8f6",
    "e1g1",
    "a7a6",
    "b5d3",
    "e7e5",
    "e3e4",
    "d5e4",
    "c3e4",
    "d8e7",
    "f1e1",
    "e8c8",
    "d1e2",
    "f6e4",
    "d3e4",
    "f7f6",
    "d2d4"
  ],
  white_remaining_ms: 40247,
  black_remaining_ms: 0,
  side_to_move: "none",
  turn_started_server_unix_ms: 1776192501862
};

const tournament: Tournament = {
  id: match.tournament_id,
  name: "Human game",
  kind: "round_robin",
  pool_id: match.pool_id,
  participant_version_ids: ["white-version", "human-player"],
  worker_count: 1,
  games_per_pairing: 1,
  status: "completed",
  started_at: "2026-04-14T18:40:00.000Z",
  completed_at: "2026-04-14T18:48:22.000Z"
};

const runningMatch: MatchSeries = {
  ...match,
  id: "running-match",
  status: "running",
  watch_state: "live",
  game_id: null,
  interactive: false,
  black_participant: {
    kind: "engine_version",
    id: "black-version",
    display_name: "Engine Black"
  }
};

const runningSnapshot: LiveMatchSnapshot = {
  match_id: runningMatch.id,
  protocol_version: 1,
  event_type: "snapshot",
  seq: 11,
  server_now_unix_ms: 1776192502055,
  status: "running",
  result: "none",
  termination: "none",
  start_fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
  fen: "rnbqkbnr/pppp1ppp/8/4p3/3P4/5N2/PPP1PPPP/RNBQKB1R b KQkq - 1 2",
  moves: ["d2d4", "e7e5", "g1f3"],
  white_remaining_ms: 4800,
  black_remaining_ms: 32000,
  side_to_move: "white",
  turn_started_server_unix_ms: 1776192502055
};

describe("WatchPage", () => {
  afterEach(() => {
    cleanup();
  });

  it("keeps showing the finished board while replay data is still loading", () => {
    mocks.useMatchesQueryMock.mockReturnValue({ data: [match] });
    mocks.useGamesQueryMock.mockReturnValue({ data: [] satisfies GameRecord[] });
    mocks.usePoolsQueryMock.mockReturnValue({
      data: [
        {
          id: match.pool_id,
          name: "Rapid Standard",
          description: null,
          variant: "standard",
          time_control: { initial_ms: 60000, increment_ms: 0 },
          fairness: { paired_games: false, swap_colors: false, opening_suite_id: null, opening_seed: null }
        }
      ]
    });
    mocks.useTournamentsQueryMock.mockReturnValue({ data: [tournament] });
    mocks.useConfirmedLiveMatchMock.mockReturnValue({
      snapshot: finishedSnapshot,
      timeline: [
        {
          seq: finishedSnapshot.seq,
          fen: finishedSnapshot.fen,
          moves: finishedSnapshot.moves,
          move_uci: finishedSnapshot.moves.at(-1) ?? null,
          white_time_left_ms: finishedSnapshot.white_remaining_ms,
          black_time_left_ms: finishedSnapshot.black_remaining_ms,
          side_to_move: finishedSnapshot.side_to_move,
          status: finishedSnapshot.status,
          result: finishedSnapshot.result,
          termination: finishedSnapshot.termination,
          server_now_unix_ms: finishedSnapshot.server_now_unix_ms,
          turn_started_server_unix_ms: finishedSnapshot.turn_started_server_unix_ms
        }
      ],
      submitMove: vi.fn(),
      error: "",
      isConnected: false
    });
    mocks.useLivePlaybackMock.mockReturnValue({
      displayedLiveFrameCount: 1,
      isLiveFollowing: true,
      matchId: match.id,
      selectedLivePly: finishedSnapshot.moves.length,
      returnToLive: vi.fn(),
      setSelectedLivePly: vi.fn()
    });
    mocks.useReplayQueryMock.mockReturnValue({ data: null });

    render(
      <MemoryRouter initialEntries={[`/watch/${match.id}`]}>
        <Routes>
          <Route path="/watch/:matchId" element={<WatchPage />} />
        </Routes>
      </MemoryRouter>
    );

    expect(screen.queryByText("Loading match viewer.")).toBeNull();
    expect(screen.getAllByText("White wins")[0]).toBeTruthy();
    expect(screen.getByText("White takes the point by Timeout.")).toBeTruthy();
    expect(screen.getByText("Replay details are loading while the final position stays on screen.")).toBeTruthy();
    expect(screen.getByText("Moves")).toBeTruthy();
  });

  it("shows critical urgency only for the active low-time side", () => {
    mocks.useMatchesQueryMock.mockReturnValue({ data: [runningMatch] });
    mocks.useGamesQueryMock.mockReturnValue({ data: [] satisfies GameRecord[] });
    mocks.usePoolsQueryMock.mockReturnValue({
      data: [
        {
          id: runningMatch.pool_id,
          name: "Rapid Standard",
          description: null,
          variant: "standard",
          time_control: { initial_ms: 60000, increment_ms: 0 },
          fairness: { paired_games: false, swap_colors: false, opening_suite_id: null, opening_seed: null }
        }
      ]
    });
    mocks.useTournamentsQueryMock.mockReturnValue({ data: [tournament] });
    mocks.useConfirmedLiveMatchMock.mockReturnValue({
      snapshot: runningSnapshot,
      timeline: [
        {
          seq: runningSnapshot.seq,
          fen: runningSnapshot.fen,
          moves: runningSnapshot.moves,
          move_uci: runningSnapshot.moves.at(-1) ?? null,
          white_time_left_ms: runningSnapshot.white_remaining_ms,
          black_time_left_ms: runningSnapshot.black_remaining_ms,
          side_to_move: runningSnapshot.side_to_move,
          status: runningSnapshot.status,
          result: runningSnapshot.result,
          termination: runningSnapshot.termination,
          server_now_unix_ms: runningSnapshot.server_now_unix_ms,
          turn_started_server_unix_ms: runningSnapshot.turn_started_server_unix_ms
        }
      ],
      submitMove: vi.fn(),
      error: "",
      isConnected: true
    });
    mocks.useLivePlaybackMock.mockReturnValue({
      displayedLiveFrameCount: 1,
      isLiveFollowing: true,
      matchId: runningMatch.id,
      selectedLivePly: runningSnapshot.moves.length,
      returnToLive: vi.fn(),
      setSelectedLivePly: vi.fn()
    });
    mocks.useReplayQueryMock.mockReturnValue({ data: null });

    render(
      <MemoryRouter initialEntries={[`/watch/${runningMatch.id}`]}>
        <Routes>
          <Route path="/watch/:matchId" element={<WatchPage />} />
        </Routes>
      </MemoryRouter>
    );

    expect(screen.getByText("White under time pressure")).toBeTruthy();
    expect(screen.getAllByText("White engine")[0].closest("[data-urgency]")?.getAttribute("data-urgency")).toBe("critical");
    expect(screen.getAllByText("Black engine")[0].closest("[data-urgency]")?.getAttribute("data-urgency")).toBe("normal");
  });
});
