import { renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useConfirmedLiveMatch } from "./live";
import type { LiveErrorMessage, LiveMatchSnapshot, LiveProtocolEvent, LiveWsServerMessage } from "./types";

const mocks = vi.hoisted(() => ({
  fetchJsonMock: vi.fn<(path: string) => Promise<LiveMatchSnapshot>>(),
  setUiDebugStateMock: vi.fn(),
  recordWsDebugMock: vi.fn()
}));

vi.mock("./api", () => ({
  fetchJson: mocks.fetchJsonMock,
  wsUrl: (path: string) => `ws://127.0.0.1:4000/api${path}`
}));

vi.mock("./debug", () => ({
  createClientActionId: () => "client-action-id",
  recordWsDebug: mocks.recordWsDebugMock,
  setUiDebugState: mocks.setUiDebugStateMock
}));

class MockWebSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;

  static instances: MockWebSocket[] = [];

  readyState = MockWebSocket.CONNECTING;
  url: string;
  sent: string[] = [];
  onopen: (() => void) | null = null;
  onmessage: ((event: MessageEvent<string>) => void) | null = null;
  onerror: (() => void) | null = null;
  onclose: ((event: CloseEvent) => void) | null = null;

  constructor(url: string) {
    this.url = url;
    MockWebSocket.instances.push(this);
  }

  send(payload: string) {
    this.sent.push(payload);
  }

  close() {
    this.readyState = MockWebSocket.CLOSED;
    this.onclose?.({
      code: 1000,
      reason: "",
      wasClean: true
    } as CloseEvent);
  }

  open() {
    this.readyState = MockWebSocket.OPEN;
    this.onopen?.();
  }

  message(payload: LiveWsServerMessage) {
    this.onmessage?.({
      data: JSON.stringify(payload)
    } as MessageEvent<string>);
  }

  static reset() {
    MockWebSocket.instances = [];
  }
}

const snapshot: LiveMatchSnapshot = {
  match_id: "1a738c18-5e65-4596-b843-62e16fe255b7",
  protocol_version: 1,
  event_type: "snapshot",
  seq: 1,
  server_now_unix_ms: 1776017184421,
  status: "running",
  result: "none",
  termination: "none",
  fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
  moves: [],
  white_remaining_ms: 60000,
  black_remaining_ms: 60000,
  side_to_move: "white",
  turn_started_server_unix_ms: 1776017184015
};

describe("useConfirmedLiveMatch", () => {
  beforeEach(() => {
    mocks.fetchJsonMock.mockReset();
    mocks.setUiDebugStateMock.mockReset();
    mocks.recordWsDebugMock.mockReset();
    MockWebSocket.reset();
    vi.stubGlobal("WebSocket", MockWebSocket);
    vi.stubGlobal("crypto", { randomUUID: vi.fn(() => "ws-connection-id") });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("recovers when a new live match reports missing state before the first snapshot is persisted", async () => {
    mocks.fetchJsonMock.mockResolvedValue(snapshot);
    const missingStateError: LiveErrorMessage = {
      message_type: "error",
      error: `live state for match ${snapshot.match_id} not found`,
      request_id: "request-id",
      ws_connection_id: "ws-connection-id"
    };

    const { result } = renderHook(() => useConfirmedLiveMatch(snapshot.match_id));

    await waitFor(() => {
      expect(MockWebSocket.instances).toHaveLength(1);
    });

    const socket = MockWebSocket.instances[0];
    socket.open();
    socket.message(missingStateError);

    await waitFor(() => {
      expect(mocks.fetchJsonMock).toHaveBeenCalledWith(`/matches/${snapshot.match_id}/live`);
      expect(result.current.error).toBe("");
      expect(result.current.snapshot?.match_id).toBe(snapshot.match_id);
      expect(result.current.snapshot?.seq).toBe(snapshot.seq);
    });
  });

  it("ignores stale websocket messages from the previous match after switching watch targets", async () => {
    const firstMatchId = "1a738c18-5e65-4596-b843-62e16fe255b7";
    const secondMatchId = "b7bb00d4-6205-43d2-b37d-152aaafe2111";
    const secondSnapshot: LiveMatchSnapshot = {
      ...snapshot,
      match_id: secondMatchId,
      seq: 4,
      moves: ["e2e4", "e7e5", "g1f3"],
      fen: "rnbqkbnr/pppp1ppp/8/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 1 2",
      side_to_move: "black"
    };

    const staleMessage: LiveProtocolEvent = {
      ...snapshot,
      match_id: firstMatchId,
      event_type: "snapshot",
      seq: 173,
      moves: Array.from({ length: 68 }, (_, index) => `m${index}`),
      fen: "5b2/1pB3p1/1R6/7r/1Pk5/3p4/1P3P1P/3K4 b - - 1 34",
      side_to_move: "black"
    };

    const { result, rerender } = renderHook(({ matchId }) => useConfirmedLiveMatch(matchId), {
      initialProps: { matchId: firstMatchId }
    });

    await waitFor(() => {
      expect(MockWebSocket.instances).toHaveLength(1);
    });

    const firstSocket = MockWebSocket.instances[0];
    firstSocket.open();

    rerender({ matchId: secondMatchId });

    await waitFor(() => {
      expect(MockWebSocket.instances).toHaveLength(2);
    });

    const secondSocket = MockWebSocket.instances[1];
    secondSocket.open();
    secondSocket.message(secondSnapshot);

    await waitFor(() => {
      expect(result.current.snapshot?.match_id).toBe(secondMatchId);
      expect(result.current.snapshot?.seq).toBe(secondSnapshot.seq);
    });

    firstSocket.message(staleMessage);

    await waitFor(() => {
      expect(result.current.snapshot?.match_id).toBe(secondMatchId);
      expect(result.current.snapshot?.seq).toBe(secondSnapshot.seq);
      expect(result.current.snapshot?.moves).toEqual(secondSnapshot.moves);
    });
  });
});
