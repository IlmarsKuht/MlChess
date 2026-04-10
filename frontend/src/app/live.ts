import { useEffect, useState } from "react";

import { apiUrl, fetchJson } from "./api";
import type { LiveMatchSnapshot, LiveProtocolEvent } from "./types";

function applyEvent(current: LiveMatchSnapshot | null, event: LiveProtocolEvent): LiveMatchSnapshot | null {
  if (current && event.seq < current.seq) {
    return current;
  }
  if (event.event_type === "snapshot") {
    if (current && event.seq === current.seq) {
      return current;
    }
    return event;
  }
  if (!current) {
    return null;
  }
  if (event.seq === current.seq) {
    return current;
  }
  switch (event.event_type) {
    case "move_committed":
      return {
        ...current,
        event_type: "snapshot" as const,
        seq: event.seq,
        server_now_unix_ms: event.server_now_unix_ms,
        status: event.status,
        fen: event.fen,
        moves: event.moves,
        white_time_left_ms: event.white_time_left_ms,
        black_time_left_ms: event.black_time_left_ms,
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
        white_time_left_ms: event.white_time_left_ms,
        black_time_left_ms: event.black_time_left_ms,
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
        white_time_left_ms: event.white_time_left_ms,
        black_time_left_ms: event.black_time_left_ms,
        side_to_move: event.side_to_move,
        turn_started_server_unix_ms: event.turn_started_server_unix_ms
      };
  }
}

export function useConfirmedLiveMatch(matchId: string) {
  const [snapshot, setSnapshot] = useState<LiveMatchSnapshot | null>(null);

  useEffect(() => {
    if (!matchId) {
      setSnapshot(null);
      return;
    }
    let cancelled = false;
    let eventSource: EventSource | null = null;
    let reconnectTimer: number | null = null;

    const close = () => {
      if (eventSource) {
        eventSource.close();
        eventSource = null;
      }
    };

    const loadSnapshot = async () => {
      const next = await fetchJson<LiveMatchSnapshot>(`/matches/${matchId}/live`);
      if (!cancelled) {
        setSnapshot((current) => applyEvent(current, next));
      }
    };

    const connect = () => {
      if (cancelled) {
        return;
      }
      close();
      const source = new EventSource(apiUrl(`/matches/${matchId}/live/stream`));
      eventSource = source;

      const handle = (event: MessageEvent<string>) => {
        try {
          const parsed = JSON.parse(event.data) as LiveProtocolEvent;
          if (!cancelled) {
            setSnapshot((current) => applyEvent(current, parsed));
          }
        } catch {
          // Ignore malformed events and wait for the next authoritative event.
        }
      };

      source.addEventListener("snapshot", handle);
      source.addEventListener("move_committed", handle);
      source.addEventListener("clock_sync", handle);
      source.addEventListener("game_finished", handle);
      source.onerror = () => {
        close();
        if (reconnectTimer === null && !cancelled) {
          reconnectTimer = window.setTimeout(() => {
            reconnectTimer = null;
            void loadSnapshot().finally(connect);
          }, 1000);
        }
      };
    };

    void loadSnapshot().finally(connect);

    return () => {
      cancelled = true;
      close();
      if (reconnectTimer !== null) {
        window.clearTimeout(reconnectTimer);
      }
    };
  }, [matchId]);

  return snapshot;
}
