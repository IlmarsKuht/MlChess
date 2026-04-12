import type { FrontendDebugState } from "./debug";

export interface DebugReportPackage {
  schema_version: number;
  generated_at: string;
  report_summary: {
    headline: string;
    route: string;
    observed_behavior: string;
    likely_failure_class: string;
    primary_ids: Record<string, string>;
    recommended_files: string[];
  };
  correlation: {
    match_id?: string;
    tournament_id?: string;
    game_id?: string;
    last_client_action_id?: string;
    last_intent_id?: string;
    ws_connection_id?: string;
    recent_request_ids: string[];
    response_request_ids: string[];
  };
  frontend: FrontendDebugState;
  backend: unknown;
  failure_analysis: {
    frontend_failure_class: string;
    retained_failure_reason?: string;
    websocket_retries: number;
    failed_request_count: number;
    latest_non_ok_request?: string;
  };
}

function collectPrimaryIds(frontend: FrontendDebugState) {
  const ids: Record<string, string> = {};
  if (frontend.selected_match_id) {
    ids.match_id = frontend.selected_match_id;
  }
  if (frontend.selected_tournament_id) {
    ids.tournament_id = frontend.selected_tournament_id;
  }
  if (frontend.selected_game_id) {
    ids.game_id = frontend.selected_game_id;
  }
  return ids;
}

function inferFailureClass(frontend: FrontendDebugState) {
  const wsErrors = frontend.recent_ws_events.filter(
    (event) => event.event === "ws.error" || event.event === "ws.server_error"
  );
  const failingLiveRequest = frontend.recent_api_requests.find(
    (request) => request.ok === false && (request.path.includes("/live") || request.path.includes("/debug"))
  );
  if (wsErrors.length >= 2 && !frontend.ws_connected) {
    return "websocket_handshake";
  }
  if (failingLiveRequest) {
    return failingLiveRequest.path.includes("/debug") ? "debug_endpoint" : "live_api";
  }
  if (frontend.last_ui_error) {
    return "frontend_state";
  }
  return "unknown";
}

function recommendedFiles(failureClass: string) {
  switch (failureClass) {
    case "websocket_handshake":
      return ["frontend/src/app/live.ts", "crates/arena-server/src/api.rs", "crates/arena-server/src/live.rs"];
    case "debug_endpoint":
      return ["frontend/src/App.tsx", "crates/arena-server/src/api.rs"];
    case "live_api":
      return ["frontend/src/app/api.ts", "crates/arena-server/src/api.rs"];
    default:
      return ["frontend/src/App.tsx", "crates/arena-server/src/api.rs"];
  }
}

export function buildDebugReportPackage(frontend: FrontendDebugState, backend: unknown): DebugReportPackage {
  const primaryIds = collectPrimaryIds(frontend);
  const failureClass = inferFailureClass(frontend);
  const failedRequests = frontend.recent_api_requests.filter((request) => request.ok === false);
  const websocketRetries = frontend.recent_ws_events.filter((event) => event.event === "ws.close").length;
  const latestFailedRequest = [...failedRequests].reverse()[0];
  const observedBehavior =
    frontend.retained_failure_reason ??
    frontend.last_ui_error ??
    (frontend.ws_connected ? "No retained failure captured." : "Live connection is currently disconnected.");

  return {
    schema_version: 1,
    generated_at: new Date().toISOString(),
    report_summary: {
      headline: `Debug report for ${frontend.route}`,
      route: frontend.route,
      observed_behavior: observedBehavior,
      likely_failure_class: failureClass,
      primary_ids: primaryIds,
      recommended_files: recommendedFiles(failureClass)
    },
    correlation: {
      ...primaryIds,
      last_client_action_id: frontend.last_client_action_id,
      last_intent_id: frontend.last_intent_id,
      ws_connection_id: frontend.ws_connection_id,
      recent_request_ids: frontend.recent_api_requests.map((request) => request.request_id),
      response_request_ids: frontend.recent_api_requests
        .map((request) => request.response_request_id)
        .filter((value): value is string => Boolean(value))
    },
    frontend,
    backend,
    failure_analysis: {
      frontend_failure_class: failureClass,
      retained_failure_reason: frontend.retained_failure_reason,
      websocket_retries: websocketRetries,
      failed_request_count: failedRequests.length,
      latest_non_ok_request: latestFailedRequest
        ? `${latestFailedRequest.method} ${latestFailedRequest.path} -> ${latestFailedRequest.status_code ?? "?"}`
        : undefined
    }
  };
}

export function suggestedDebugReportFilename(report: DebugReportPackage) {
  const entityId =
    report.correlation.match_id ?? report.correlation.game_id ?? report.correlation.tournament_id ?? "general";
  return `mlchess-bug-report-${report.generated_at.replaceAll(":", "-")}-${entityId}.json`;
}

export function clipboardDebugReportSummary(savedPath: string, report: DebugReportPackage) {
  return [
    `Saved report: ${savedPath}`,
    `Route: ${report.report_summary.route}`,
    `Likely failure class: ${report.report_summary.likely_failure_class}`,
    `Observed behavior: ${report.report_summary.observed_behavior}`,
    `Primary IDs: ${Object.entries(report.report_summary.primary_ids)
      .map(([key, value]) => `${key}=${value}`)
      .join(", ") || "none"}`,
    `Suggested files: ${report.report_summary.recommended_files.join(", ")}`
  ].join("\n");
}
