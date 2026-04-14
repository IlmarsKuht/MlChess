import { Outlet, useLocation, useNavigate } from "react-router-dom";

import { useFlash } from "../providers/FlashProvider";
import { workspaceViews, type WorkspaceView } from "../routes/config";
import { DebugDrawer } from "../../features/debug/DebugDrawer";
import { useArenaSummaryQueries } from "../../shared/queries/arena";
import { MetricCard, StatusBadge } from "../../shared/ui";

function activeViewFromPath(pathname: string): WorkspaceView {
  if (pathname.startsWith("/setup") || pathname.startsWith("/engine/")) {
    return "setup";
  }
  if (pathname.startsWith("/live-duel")) {
    return "live-duel";
  }
  if (pathname.startsWith("/play-engine")) {
    return "play-engine";
  }
  if (pathname.startsWith("/events")) {
    return "events";
  }
  if (pathname.startsWith("/tournaments")) {
    return "tournaments";
  }
  if (pathname.startsWith("/replay")) {
    return "replay";
  }
  return "overview";
}

export function AppLayout() {
  const location = useLocation();
  const navigate = useNavigate();
  const activeView = activeViewFromPath(location.pathname);
  const { notice, error } = useFlash();
  const { agents, versions, games, tournaments } = useArenaSummaryQueries(true);
  const documentedVersions = versions.data?.filter((version) => Boolean(version.documentation?.trim())) ?? [];
  const runningTournaments = tournaments.data?.filter((tournament) => tournament.status === "running") ?? [];

  return (
    <div className="shell">
      <header className="hero">
        <div className="hero-copy">
          <p className="eyebrow">Rust Chess Arena</p>
          <h1>Watch engines face off without wading through control panels.</h1>
          <p className="lede">
            Browse the engines, launch backend-defined events, and jump into finished games from a cleaner live and
            replay viewer.
          </p>
          <div className="hero-statuses">
            <StatusBadge tone={runningTournaments.length > 0 ? "running" : "quiet"}>
              {runningTournaments.length > 0
                ? `${runningTournaments.length} event${runningTournaments.length === 1 ? "" : "s"} live`
                : "Arena idle"}
            </StatusBadge>
            <StatusBadge tone={documentedVersions.length > 0 ? "good" : "warning"}>
              {documentedVersions.length > 0 ? "Engine docs available" : "Engine docs missing"}
            </StatusBadge>
          </div>
        </div>
        <div className="hero-metrics">
          <MetricCard label="Families" value={String(agents.data?.length ?? 0)} />
          <MetricCard label="Engines" value={String(versions.data?.length ?? 0)} />
          <MetricCard label="Finished Games" value={String(games.data?.length ?? 0)} />
        </div>
      </header>

      <nav className="view-switcher" aria-label="Arena sections">
        {workspaceViews.map((view) => (
          <button
            key={view.id}
            type="button"
            className={`view-button ${activeView === view.id ? "view-button-active" : ""}`}
            onClick={() => navigate(view.path)}
          >
            <span>{view.label}</span>
            <small>{view.detail}</small>
          </button>
        ))}
      </nav>

      {(notice || error) && (
        <section className={`banner ${error ? "banner-error" : "banner-ok"}`}>{error || notice}</section>
      )}

      <main className={`workspace view-${activeView}`}>
        <DebugDrawer />
        <Outlet />
      </main>
    </div>
  );
}
