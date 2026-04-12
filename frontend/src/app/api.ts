import {
  createRequestId,
  currentRoute,
  recordApiDebugFinish,
  recordApiDebugStart
} from "./debug";

const apiBase = import.meta.env.VITE_API_BASE ?? "/api";

export function apiUrl(path: string) {
  return `${apiBase}${path}`;
}

export function wsUrl(path: string) {
  const base = apiBase.startsWith("http")
    ? new URL(apiBase)
    : new URL(apiBase, window.location.origin);
  base.protocol = base.protocol === "https:" ? "wss:" : "ws:";
  base.pathname = `${base.pathname.replace(/\/$/, "")}${path}`;
  base.search = "";
  base.hash = "";
  return base.toString();
}

export interface DebugRequestInit extends RequestInit {
  debug?: {
    clientActionId?: string;
  };
}

export async function fetchJson<T>(path: string, init?: DebugRequestInit): Promise<T> {
  const requestId = createRequestId();
  const startedAt = new Date().toISOString();
  const startedMs = Date.now();
  const { debug, headers, method, ...rest } = init ?? {};
  const finalMethod = method ?? "GET";
  recordApiDebugStart({
    request_id: requestId,
    client_action_id: debug?.clientActionId,
    method: finalMethod,
    path,
    route: currentRoute(),
    started_at: startedAt
  });
  const response = await fetch(apiUrl(path), {
    headers: {
      "Content-Type": "application/json",
      "x-request-id": requestId,
      "x-client-route": currentRoute(),
      "x-client-ts": startedAt,
      ...(debug?.clientActionId ? { "x-client-action-id": debug.clientActionId } : {}),
      ...(headers ?? {})
    },
    method: finalMethod,
    ...rest
  });
  recordApiDebugFinish(requestId, {
    completed_at: new Date().toISOString(),
    duration_ms: Date.now() - startedMs,
    status_code: response.status,
    ok: response.ok,
    response_request_id: response.headers.get("x-request-id") ?? undefined
  });
  if (!response.ok) {
    const error = await response.json().catch(() => ({ error: "Request failed" }));
    recordApiDebugFinish(requestId, {
      error: error.error ?? "Request failed"
    });
    throw new Error(error.error ?? "Request failed");
  }
  return response.json();
}
