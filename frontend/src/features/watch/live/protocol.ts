import type {
  GameTermination,
  LiveErrorMessage,
  LiveIntentAck,
  LiveMatchSnapshot,
  LiveProtocolEvent,
  LiveResult,
  LiveStatus,
  LiveWsServerMessage,
  ProtocolLiveSide
} from "../../../shared/api/types";

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

export interface ConfirmedLiveState {
  snapshot: LiveMatchSnapshot | null;
  timeline: ConfirmedLiveFrame[];
}

export function frameFromSnapshot(snapshot: LiveMatchSnapshot): ConfirmedLiveFrame {
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

export function snapshotFromEvent(current: LiveMatchSnapshot, event: LiveProtocolEvent): LiveMatchSnapshot {
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

export function isProtocolEvent(message: LiveWsServerMessage): message is LiveProtocolEvent {
  return "event_type" in message;
}

export function isIntentAck(message: LiveWsServerMessage): message is LiveIntentAck {
  return "message_type" in message && message.message_type === "intent_ack";
}

export function isErrorMessage(message: LiveWsServerMessage): message is LiveErrorMessage {
  return "message_type" in message && message.message_type === "error";
}

export function isMissingLiveStateError(error: string) {
  return /^live state for match [0-9a-f-]+ not found$/i.test(error.trim());
}
