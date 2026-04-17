import {
  createApiClient,
  apiUrl,
  wsUrl,
  type ApiRequestInit
} from "../shared/api/client";
import {
  createRequestId,
  currentRoute,
  recordApiDebugFinish,
  recordApiDebugStart
} from "./debug";

const instrumentedClient = createApiClient({
  beforeRequest({ path, method, debug }) {
    const requestId = createRequestId();
    const startedAt = new Date().toISOString();
    const route = currentRoute();
    recordApiDebugStart({
      request_id: requestId,
      client_action_id: debug?.clientActionId,
      method,
      path,
      route,
      started_at: startedAt
    });
    return {
      requestId,
      startedAt,
      headers: {
        "x-request-id": requestId,
        "x-client-route": route,
        "x-client-ts": startedAt,
        ...(debug?.clientActionId ? { "x-client-action-id": debug.clientActionId } : {})
      }
    };
  },
  afterResponse({ requestId, completedAt, durationMs, statusCode, ok, responseRequestId }) {
    if (!requestId) {
      return;
    }
    recordApiDebugFinish(requestId, {
      completed_at: completedAt,
      duration_ms: durationMs,
      status_code: statusCode,
      ok,
      response_request_id: responseRequestId
    });
  },
  afterError({ requestId, error }) {
    if (!requestId) {
      return;
    }
    recordApiDebugFinish(requestId, { error });
  }
});

export { apiUrl, wsUrl };
export type { ApiRequestInit };

export function fetchJson<T>(path: string, init?: ApiRequestInit): Promise<T> {
  return instrumentedClient.fetchJson<T>(path, init);
}
