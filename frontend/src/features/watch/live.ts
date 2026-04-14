import { useEffect, useRef, useState } from "react";

import { createClientActionId, recordWsDebug, setUiDebugState } from "../../app/debug";
import type {
  GameTermination,
  LiveErrorMessage,
  LiveIntentAck,
  LiveMatchSnapshot,
  LiveProtocolEvent,
  LiveResult,
  LiveStatus,
  LiveSubmitMoveMessage,
  LiveWsClientMessage,
  LiveWsServerMessage,
  ProtocolLiveSide
} from "../../app/types";
import { fetchJson, wsUrl } from "../../app/api";

export interface ConfirmedLiveFrame {
  seq: number;
  fen: string;
  moves: string[];
  move_uci: string | null;
  white_time_left_ms: number;
  black_time_left_ms: number;
  side_to_move: ProtocolLiveSide;
  status: LiveStatus;
  result: LiveResult;
  termination: GameTermination;
  server_now_unix_ms: number;
  turn_started_server_unix_ms: number;
}

interface ConfirmedLiveState {
  snapshot: LiveMatchSnapshot | null;
  timeline: ConfirmedLiveFrame[];
}

const transientMissingLiveStateRetryLimit = 5;

type ReduceResult =
  | { kind: "noop"; state: ConfirmedLiveState | null }
  | { kind: "next"; state: ConfirmedLiveState }
  | { kind: "gap" };

function frameFromSnapshot(snapshot: LiveMatchSnapshot): ConfirmedLiveFrame {
  return {
    seq: snapshot.seq,
    fen: snapshot.fen,
    moves: snapshot.moves,
    move_uci: snapshot.moves.at(-1) ?? null,
    white_time_left_ms: snapshot.white_remaining_ms,
    black_time_left_ms: snapshot.black_remaining_ms,
    side_to_move: snapshot.side_to_move,
    status: snapshot.status,
    result: snapshot.result,
    termination: snapshot.termination,
    server_now_unix_ms: snapshot.server_now_unix_ms,
    turn_started_server_unix_ms: snapshot.turn_started_server_unix_ms
  };
}

function snapshotFromEvent(current: LiveMatchSnapshot, event: LiveProtocolEvent): LiveMatchSnapshot {
  switch (event.event_type) {
    case "snapshot":
      return event;
    case "move_committed":
      return {
        ...current,
        event_type: "snapshot" as const,
        seq: event.seq,
        server_now_unix_ms: event.server_now_unix_ms,
        status: event.status,
        fen: event.fen,
        moves: event.moves,
        white_remaining_ms: event.white_remaining_ms,
        black_remaining_ms: event.black_remaining_ms,
        side_to_move: event.side_to_move,
        turn_started_server_unix_ms: event.turn_started_server_unix_ms
      };
    case "clock_sync":
      return {
        ...current,
        event_type: "snapshot" as const,
        seq: event.seq,
        server_now_unix_ms: event.server_now_unix_ms,
        status: event.status,
        white_remaining_ms: event.white_remaining_ms,
        black_remaining_ms: event.black_remaining_ms,
        side_to_move: event.side_to_move,
        turn_started_server_unix_ms: event.turn_started_server_unix_ms
      };
    case "game_finished":
      return {
        ...current,
        event_type: "snapshot" as const,
        seq: event.seq,
        server_now_unix_ms: event.server_now_unix_ms,
        status: event.status,
        result: event.result,
        termination: event.termination,
        fen: event.fen,
        moves: event.moves,
        white_remaining_ms: event.white_remaining_ms,
        black_remaining_ms: event.black_remaining_ms,
        side_to_move: event.side_to_move,
        turn_started_server_unix_ms: event.turn_started_server_unix_ms
      };
  }
}

function isProtocolEvent(message: LiveWsServerMessage): message is LiveProtocolEvent {
  return "event_type" in message;
}

function isIntentAck(message: LiveWsServerMessage): message is LiveIntentAck {
  return "message_type" in message && message.message_type === "intent_ack";
}

function isErrorMessage(message: LiveWsServerMessage): message is LiveErrorMessage {
  return "message_type" in message && message.message_type === "error";
}

function isMissingLiveStateError(error: string) {
  return /^live state for match [0-9a-f-]+ not found$/i.test(error.trim());
}

function reduceEvent(current: ConfirmedLiveState | null, event: LiveProtocolEvent): ReduceResult {
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

export function useConfirmedLiveMatch(matchId: string) {
  const [state, setState] = useState<ConfirmedLiveState | null>(null);
  const [error, setError] = useState("");
  const [isConnected, setIsConnected] = useState(false);
  const stateRef = useRef<ConfirmedLiveState | null>(null);
  const socketRef = useRef<WebSocket | null>(null);
  const reconnectTimerRef = useRef<number | null>(null);
  const wsConnectionIdRef = useRef<string | null>(null);
  const connectAttemptRef = useRef(0);
  const missingLiveStateRetryCountRef = useRef(0);

  useEffect(() => {
    if (!matchId) {
      setState(null);
      stateRef.current = null;
      setError("");
      setIsConnected(false);
      missingLiveStateRetryCountRef.current = 0;
      return;
    }

    let cancelled = false;

    const clearReconnect = () => {
      if (reconnectTimerRef.current !== null) {
        window.clearTimeout(reconnectTimerRef.current);
        reconnectTimerRef.current = null;
      }
    };

    const closeSocket = () => {
      if (socketRef.current) {
        socketRef.current.close();
        socketRef.current = null;
      }
    };

    const loadSnapshot = async () => {
      const next = await fetchJson<LiveMatchSnapshot>(`/matches/${matchId}/live`);
      if (cancelled) {
        return;
      }
      missingLiveStateRetryCountRef.current = 0;
      setError("");
      setState((current) => {
        const result = reduceEvent(current, next);
        const nextState = result.kind === "next" ? result.state : current;
        stateRef.current = nextState;
        return nextState;
      });
    };

    const scheduleReconnect = () => {
      if (cancelled || reconnectTimerRef.current !== null) {
        return;
      }
      reconnectTimerRef.current = window.setTimeout(() => {
        reconnectTimerRef.current = null;
        void connect();
      }, 1000);
    };

    const handleProtocolEvent = (event: LiveProtocolEvent) => {
      const result = reduceEvent(stateRef.current, event);
      if (result.kind === "gap") {
        void loadSnapshot();
        return;
      }
      if (result.kind === "next") {
        missingLiveStateRetryCountRef.current = 0;
        stateRef.current = result.state;
        setState(result.state);
        setUiDebugState({
          current_snapshot_seq: result.state.snapshot?.seq,
          current_live_status: result.state.snapshot?.status,
          live_summary: result.state.snapshot
            ? `seq ${result.state.snapshot.seq} ${result.state.snapshot.status} ${result.state.snapshot.side_to_move}`
            : undefined
        });
      }
    };

    const connect = async () => {
      if (cancelled) {
        return;
      }
      clearReconnect();
      closeSocket();

      const liveWsUrl = wsUrl(`/matches/${matchId}/live/ws`);
      connectAttemptRef.current += 1;
      const socket = new WebSocket(liveWsUrl);
      socketRef.current = socket;
      wsConnectionIdRef.current = crypto.randomUUID();

      socket.onopen = () => {
        if (cancelled) {
          return;
        }
        setIsConnected(true);
        setError("");
        setUiDebugState({ ws_connected: true, ws_connection_id: wsConnectionIdRef.current ?? undefined });
        recordWsDebug({
          at: new Date().toISOString(),
          event: "ws.open",
          match_id: matchId,
          url: liveWsUrl,
          attempt: connectAttemptRef.current,
          ws_connection_id: wsConnectionIdRef.current ?? undefined
        });
        const subscribe: LiveWsClientMessage = {
          message_type: "subscribe",
          ws_connection_id: wsConnectionIdRef.current ?? undefined,
          ...(stateRef.current?.snapshot?.seq !== undefined ? { last_seq: stateRef.current.snapshot.seq } : {})
        };
        socket.send(JSON.stringify(subscribe));
      };

      socket.onmessage = (messageEvent) => {
        try {
          const message = JSON.parse(messageEvent.data) as LiveWsServerMessage;
          recordWsDebug({
            at: new Date().toISOString(),
            event: "ws.message",
            match_id: matchId,
            url: liveWsUrl,
            attempt: connectAttemptRef.current,
            ws_connection_id: wsConnectionIdRef.current ?? undefined,
            payload: message
          });
          if (isProtocolEvent(message)) {
            handleProtocolEvent(message);
            return;
          }
          if (isErrorMessage(message)) {
            const shouldRetryMissingState =
              !stateRef.current?.snapshot &&
              isMissingLiveStateError(message.error) &&
              missingLiveStateRetryCountRef.current < transientMissingLiveStateRetryLimit;
            if (shouldRetryMissingState) {
              missingLiveStateRetryCountRef.current += 1;
              void loadSnapshot().catch(() => {
                // The match may not have published its first snapshot yet.
              });
              socket.close();
              return;
            }
            setError(message.error);
            setUiDebugState({
              last_ui_error: message.error,
              last_client_action_id: message.client_action_id,
              ws_connection_id: message.ws_connection_id ?? wsConnectionIdRef.current ?? undefined
            });
            recordWsDebug({
              at: new Date().toISOString(),
              event: "ws.server_error",
              match_id: matchId,
              url: liveWsUrl,
              attempt: connectAttemptRef.current,
              ws_connection_id: message.ws_connection_id ?? wsConnectionIdRef.current ?? undefined,
              client_action_id: message.client_action_id,
              request_id: message.request_id,
              payload: message
            });
            return;
          }
          if (isIntentAck(message) && message.ack === "duplicate") {
            setError("Move was already submitted.");
          }
          if (isIntentAck(message)) {
            setUiDebugState({
              last_intent_id: message.intent_id,
              last_client_action_id: message.client_action_id,
              ws_connection_id: message.ws_connection_id ?? wsConnectionIdRef.current ?? undefined
            });
          }
        } catch {
          // Ignore malformed websocket payloads and wait for the next message.
        }
      };

      socket.onerror = () => {
        setIsConnected(false);
        setUiDebugState({ ws_connected: false });
      };

      socket.onclose = (event) => {
        setIsConnected(false);
        socketRef.current = null;
        setUiDebugState({ ws_connected: false });
        recordWsDebug({
          at: new Date().toISOString(),
          event: "ws.close",
          match_id: matchId,
          url: liveWsUrl,
          attempt: connectAttemptRef.current,
          ws_connection_id: wsConnectionIdRef.current ?? undefined,
          close_code: event.code,
          close_reason: event.reason || undefined,
          was_clean: event.wasClean
        });
        if (!cancelled) {
          if (!stateRef.current?.snapshot) {
            void loadSnapshot().catch(() => {
              // Snapshot bootstrap is best-effort; reconnect loop keeps trying the socket.
            });
          }
          scheduleReconnect();
        }
      };
    };

    void connect();

    return () => {
      cancelled = true;
      clearReconnect();
      closeSocket();
      stateRef.current = null;
      setIsConnected(false);
    };
  }, [matchId]);

  const submitMove = async (move_uci: string) => {
    const socket = socketRef.current;
    if (!socket || socket.readyState !== WebSocket.OPEN) {
      throw new Error("Live connection is not ready");
    }
    const payload: LiveSubmitMoveMessage = {
      message_type: "submit_move",
      intent_id: crypto.randomUUID(),
      client_action_id: createClientActionId(),
      ws_connection_id: wsConnectionIdRef.current ?? undefined,
      move_uci
    };
    setUiDebugState({
      last_intent_id: payload.intent_id,
      last_client_action_id: payload.client_action_id,
      ws_connection_id: wsConnectionIdRef.current ?? undefined
    });
    recordWsDebug({
      at: new Date().toISOString(),
      event: "ws.submit_move",
      match_id: matchId,
      url: socket.url,
      ws_connection_id: wsConnectionIdRef.current ?? undefined,
      client_action_id: payload.client_action_id,
      intent_id: payload.intent_id,
      payload
    });
    socket.send(JSON.stringify(payload));
  };

  return {
    snapshot: state?.snapshot ?? null,
    timeline: state?.timeline ?? [],
    submitMove,
    error,
    isConnected
  };
}
