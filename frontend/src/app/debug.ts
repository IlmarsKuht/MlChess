export interface ApiDebugRecord {
  request_id: string;
  client_action_id?: string;
  method: string;
  path: string;
  route: string;
  started_at: string;
  completed_at?: string;
  duration_ms?: number;
  status_code?: number;
  ok?: boolean;
  response_request_id?: string;
  error?: string;
}

export interface WsDebugRecord {
  at: string;
  event: string;
  match_id?: string;
  url?: string;
  attempt?: number;
  ws_connection_id?: string;
  client_action_id?: string;
  intent_id?: string;
  request_id?: string;
  close_code?: number;
  close_reason?: string;
  was_clean?: boolean;
  payload?: unknown;
}

export interface FrontendDebugState {
  schema_version: number;
  enabled: boolean;
  route: string;
  selected_match_id?: string;
  selected_tournament_id?: string;
  selected_game_id?: string;
  ws_connected: boolean;
  ws_connection_id?: string;
  current_snapshot_seq?: number;
  current_live_status?: string;
  live_summary?: string;
  last_ui_error?: string;
  last_intent_id?: string;
  last_client_action_id?: string;
  retained_failure_reason?: string;
  retained_failure_at?: string;
  recent_api_requests: ApiDebugRecord[];
  recent_ws_events: WsDebugRecord[];
}

const DEBUG_STORAGE_KEY = "mlchess-debug-drawer";
const API_LIMIT = 10;
const WS_LIMIT = 20;
const listeners = new Set<() => void>();

let state: FrontendDebugState = {
  schema_version: 2,
  enabled: readStoredEnabled(),
  route: currentRoute(),
  ws_connected: false,
  recent_api_requests: [],
  recent_ws_events: []
};

export function currentRoute() {
  const hash = window.location.hash || "#/";
  return `${window.location.pathname}${window.location.search}${hash}`;
}

function readStoredEnabled() {
  try {
    return window.localStorage.getItem(DEBUG_STORAGE_KEY) === "1";
  } catch {
    return false;
  }
}

function pushLimited<T>(items: T[], next: T, limit: number) {
  const merged = [...items, next];
  return merged.slice(Math.max(merged.length - limit, 0));
}

function emit() {
  for (const listener of listeners) {
    listener();
  }
}

function update(next: Partial<FrontendDebugState>) {
  const changed = Object.entries(next).some(([key, value]) => !Object.is(state[key as keyof FrontendDebugState], value));
  if (!changed) {
    return;
  }
  state = {
    ...state,
    ...next
  };
  emit();
}

export function subscribeDebug(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function getDebugState() {
  return state;
}

export function setDebugEnabled(enabled: boolean) {
  if (state.enabled === enabled) {
    return;
  }
  state = {
    ...state,
    enabled
  };
  try {
    window.localStorage.setItem(DEBUG_STORAGE_KEY, enabled ? "1" : "0");
  } catch {
    // Ignore storage issues in restricted contexts.
  }
  emit();
}

export function toggleDebugEnabled() {
  setDebugEnabled(!state.enabled);
}

export function syncRouteDebugState() {
  update({ route: currentRoute() });
}

export function setUiDebugState(next: Partial<FrontendDebugState>) {
  update(next);
}

export function createRequestId() {
  return crypto.randomUUID();
}

export function createClientActionId() {
  return crypto.randomUUID();
}

export function recordApiDebugStart(record: ApiDebugRecord) {
  update({
    recent_api_requests: pushLimited(state.recent_api_requests, record, API_LIMIT),
    last_client_action_id: record.client_action_id ?? state.last_client_action_id
  });
}

export function recordApiDebugFinish(requestId: string, patch: Partial<ApiDebugRecord>) {
  let failurePatch: Partial<FrontendDebugState> | null = null;
  const next = state.recent_api_requests.map((record) => {
    if (record.request_id !== requestId) {
      return record;
    }
    const merged = { ...record, ...patch };
    if (
      merged.ok === false &&
      (merged.status_code === undefined || merged.status_code >= 500 || merged.path.includes("/debug/") || merged.path.includes("/live"))
    ) {
      failurePatch = {
        retained_failure_at: merged.completed_at,
        retained_failure_reason: `${merged.method} ${merged.path} failed${merged.status_code ? ` with ${merged.status_code}` : ""}${
          merged.error ? `: ${merged.error}` : ""
        }`
      };
    }
    return merged;
  });
  update({ recent_api_requests: next, ...(failurePatch ?? {}) });
}

export function recordWsDebug(event: WsDebugRecord) {
  const updates: Partial<FrontendDebugState> = {
    recent_ws_events: pushLimited(state.recent_ws_events, event, WS_LIMIT),
    last_client_action_id: event.client_action_id ?? state.last_client_action_id,
    last_intent_id: event.intent_id ?? state.last_intent_id,
    ws_connection_id: event.ws_connection_id ?? state.ws_connection_id
  };
  if (
    event.event === "ws.error" ||
    event.event === "ws.server_error" ||
    (event.event === "ws.close" && event.close_code !== undefined && event.close_code !== 1000)
  ) {
    updates.retained_failure_at = event.at;
    updates.retained_failure_reason =
      event.event === "ws.server_error"
        ? `Websocket server error${typeof event.payload === "object" && event.payload ? "" : ""}`
        : `Websocket ${event.event.replace("ws.", "")} for ${event.match_id ?? "unknown match"}`;
  }
  update(updates);
}
