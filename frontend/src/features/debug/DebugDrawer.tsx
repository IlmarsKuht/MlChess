import { useSyncExternalStore } from "react";
import { useLocation, useSearchParams } from "react-router-dom";

import { buildDebugReportPackage, clipboardDebugReportSummary, suggestedDebugReportFilename } from "../../app/debug-report";
import { createClientActionId, getDebugState, subscribeDebug } from "../../app/debug";
import { fetchJson } from "../../shared/api/client";
import { loadErrorMessage } from "../../shared/lib/errors";

export function DebugDrawer() {
  const location = useLocation();
  const [searchParams] = useSearchParams();
  const debugState = useSyncExternalStore(subscribeDebug, getDebugState, getDebugState);

  const selectedGameId = searchParams.get("gameId") ?? undefined;
  const selectedMatchId = location.pathname.startsWith("/watch/") ? location.pathname.split("/").at(-1) : undefined;

  if (!debugState.enabled) {
    return null;
  }

  async function copyDebugBundle() {
    const frontendBundle = getDebugState();
    const clientActionId = createClientActionId();
    let backendBundle: unknown = null;
    let backendError = "";

    try {
      if (selectedGameId) {
        backendBundle = await fetchJson(`/debug/games/${selectedGameId}/bundle`, {
          debug: { clientActionId }
        });
      } else if (selectedMatchId) {
        backendBundle = await fetchJson(`/debug/matches/${selectedMatchId}/bundle`, {
          debug: { clientActionId }
        });
      }
    } catch (bundleError) {
      backendError = loadErrorMessage(bundleError);
    }

    const report = buildDebugReportPackage(
      {
        ...frontendBundle,
        retained_failure_reason:
          backendError && !frontendBundle.retained_failure_reason
            ? `Backend debug bundle fetch failed: ${backendError}`
            : frontendBundle.retained_failure_reason
      },
      backendBundle
    );

    try {
      const saveResponse = await fetchJson<{ path: string }>(`/debug/reports`, {
        method: "POST",
        debug: { clientActionId },
        body: JSON.stringify({
          preferred_filename: suggestedDebugReportFilename(report),
          report
        })
      });
      await navigator.clipboard.writeText(clipboardDebugReportSummary(saveResponse.path, report));
    } catch (saveError) {
      const fallbackBundle = {
        summary: `Frontend route ${frontendBundle.route}${backendError ? ` | backend bundle unavailable: ${backendError}` : ""}`,
        report
      };
      await navigator.clipboard.writeText(JSON.stringify(fallbackBundle, null, 2));
      throw new Error(`Debug report save failed: ${loadErrorMessage(saveError)}. Copied JSON bundle instead.`);
    }
  }

  return (
    <aside className="panel debug-drawer">
      <div className="panel-header">
        <h2>Debug Drawer</h2>
        <span>Ctrl+Shift+D</span>
      </div>
      <div className="stack debug-drawer-copy">
        <div className="result-strip">
          <strong>Route</strong>
          <span>{debugState.route}</span>
        </div>
        <div className="result-strip">
          <strong>IDs</strong>
          <span>
            match {debugState.selected_match_id ?? "n/a"} • tournament {debugState.selected_tournament_id ?? "n/a"} •
            game {debugState.selected_game_id ?? "n/a"}
          </span>
        </div>
        <div className="result-strip">
          <strong>Live</strong>
          <span>
            {debugState.ws_connected ? "connected" : "disconnected"} • ws {debugState.ws_connection_id ?? "n/a"} • seq{" "}
            {debugState.current_snapshot_seq ?? "n/a"}
          </span>
        </div>
        <div className="result-strip">
          <strong>Last UI Error</strong>
          <span>{debugState.last_ui_error ?? "none"}</span>
        </div>
        <div className="result-strip">
          <strong>Retained Failure</strong>
          <span>{debugState.retained_failure_reason ?? "none"}</span>
        </div>
        <div className="result-strip">
          <strong>Summary</strong>
          <span>{debugState.live_summary ?? "no live summary"}</span>
        </div>
        <button type="button" onClick={() => void copyDebugBundle()}>
          Export Debug Report
        </button>
        <div className="section-heading">Recent API Requests</div>
        <div className="table">
          {debugState.recent_api_requests.map((request) => (
            <div className="table-row table-row-stack" key={request.request_id}>
              <div>
                <strong>
                  {request.method} {request.path}
                </strong>
                <p>
                  request {request.request_id} • action {request.client_action_id ?? "n/a"}
                </p>
              </div>
              <div className="chip">{request.status_code ?? "..."}</div>
            </div>
          ))}
        </div>
        <div className="section-heading">Recent Websocket Events</div>
        <div className="table">
          {debugState.recent_ws_events.map((event, index) => (
            <div className="table-row table-row-stack" key={`${event.at}-${event.event}-${index}`}>
              <div>
                <strong>{event.event}</strong>
                <p>
                  ws {event.ws_connection_id ?? "n/a"} • action {event.client_action_id ?? "n/a"} • intent{" "}
                  {event.intent_id ?? "n/a"}
                </p>
              </div>
              <div className="chip">{new Date(event.at).toLocaleTimeString()}</div>
            </div>
          ))}
        </div>
      </div>
    </aside>
  );
}
