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

export interface ApiRequestDebug {
  clientActionId?: string;
}

export interface ApiRequestInit extends RequestInit {
  debug?: {
    clientActionId?: string;
  };
}

export interface ApiClientHooks {
  beforeRequest?: (request: {
    path: string;
    method: string;
    debug?: ApiRequestDebug;
  }) => {
    headers?: Record<string, string>;
    requestId?: string;
    startedAt?: string;
  } | void;
  afterResponse?: (result: {
    requestId?: string;
    startedAt?: string;
    completedAt: string;
    durationMs: number;
    statusCode: number;
    ok: boolean;
    responseRequestId?: string;
  }) => void;
  afterError?: (result: { requestId?: string; error: string }) => void;
}

export function createApiClient(hooks: ApiClientHooks = {}) {
  return {
    fetchJson<T>(path: string, init?: ApiRequestInit) {
      return fetchJson<T>(path, init, hooks);
    }
  };
}

export async function fetchJson<T>(path: string, init?: ApiRequestInit, hooks: ApiClientHooks = {}): Promise<T> {
  const startedAtFallback = new Date().toISOString();
  const startedMs = Date.now();
  const { debug, headers, method, ...rest } = init ?? {};
  const finalMethod = method ?? "GET";
  const hookContext = hooks.beforeRequest?.({
    path,
    method: finalMethod,
    debug
  });
  const requestId = hookContext?.requestId;
  const startedAt = hookContext?.startedAt ?? startedAtFallback;
  const response = await fetch(apiUrl(path), {
    headers: {
      "Content-Type": "application/json",
      ...(hookContext?.headers ?? {}),
      ...(headers ?? {})
    },
    method: finalMethod,
    ...rest
  });
  const completedAt = new Date().toISOString();
  hooks.afterResponse?.({
    requestId,
    startedAt,
    completedAt,
    durationMs: Date.now() - startedMs,
    statusCode: response.status,
    ok: response.ok,
    responseRequestId: response.headers.get("x-request-id") ?? undefined
  });
  if (!response.ok) {
    const error = await response.json().catch(() => ({ error: "Request failed" }));
    const message = error.error ?? "Request failed";
    hooks.afterError?.({ requestId, error: message });
    throw new Error(message);
  }
  return response.json();
}
