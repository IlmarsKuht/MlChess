import { useNavigate } from "react-router-dom";

import { useEventPresetsQuery, useGamesQuery, useLeaderboardQuery, useMatchesQuery, usePoolsQuery, useTournamentsQuery, useAgentVersionsQuery } from "../../shared/queries/arena";
import { formatLabel, formatTimeControl, formatVariant, matchResultText } from "../../shared/lib/format";
import { participantName } from "../../shared/lib/participants";
import { EmptyState, RouteErrorState, RouteLoadingState, StatusBadge } from "../../shared/ui";

export function OverviewPage() {
  const navigate = useNavigate();
  const pools = usePoolsQuery(3000);
  const eventPresets = useEventPresetsQuery(3000);
  const tournaments = useTournamentsQuery(3000);
  const leaderboard = useLeaderboardQuery(undefined, 3000);
  const games = useGamesQuery(3000);
  const versions = useAgentVersionsQuery();

  if (pools.isLoading || eventPresets.isLoading || tournaments.isLoading || leaderboard.isLoading || games.isLoading || versions.isLoading) {
    return <RouteLoadingState message="Loading arena overview..." />;
  }

  const error = pools.error ?? eventPresets.error ?? tournaments.error ?? leaderboard.error ?? games.error ?? versions.error;
  if (error) {
    return <RouteErrorState message={error.message} />;
  }

  const documentedVersions = versions.data?.filter((version) => Boolean(version.documentation?.trim())) ?? [];
  const activePool = pools.data?.[0];
  const recentGames = games.data?.slice(0, 6) ?? [];
  const runningTournaments = tournaments.data?.filter((tournament) => tournament.status === "running") ?? [];
  const completedTournaments = tournaments.data?.filter((tournament) => tournament.status === "completed") ?? [];

  return (
    <>
      <section className="panel panel-spotlight">
        <div className="panel-header">
          <h2>Overview</h2>
          <StatusBadge tone={runningTournaments.length > 0 ? "running" : "quiet"}>
            {runningTournaments.length > 0 ? "Live now" : "Ready"}
          </StatusBadge>
        </div>
        <p className="panel-copy">
          Check what is live, browse featured engines, and jump straight into duels, the tournament map, or
          finished games. The fastest paths below are tuned for “watch something now” and “review the latest result.”
        </p>
        <div className="summary-grid">
          <div className="summary-card">
            <span>Documented engines</span>
            <strong>{documentedVersions.length}</strong>
            <p>{documentedVersions.length > 0 ? "Deep docs ready on engine pages" : "No detailed docs yet"}</p>
          </div>
          <div className="summary-card">
            <span>Featured format</span>
            <strong>{activePool?.name ?? "No format yet"}</strong>
            <p>
              {activePool
                ? `${formatVariant(activePool.variant)} • ${formatTimeControl(activePool.time_control)}`
                : "Choose a format to browse the pool list"}
            </p>
          </div>
          <div className="summary-card">
            <span>Event presets</span>
            <strong>{eventPresets.data?.length ?? 0}</strong>
            <p>
              {completedTournaments.length} completed runs, {runningTournaments.length} running
            </p>
          </div>
        </div>
        <div className="quick-actions">
          <button type="button" className="button-ghost" onClick={() => navigate("/setup")}>
            Browse engines
          </button>
          <button type="button" className="button-ghost" onClick={() => navigate("/live-duel")}>
            Watch live duel
          </button>
          <button type="button" className="button-ghost" onClick={() => navigate("/play-engine")}>
            Play vs engine
          </button>
          <button type="button" className="button-ghost" onClick={() => navigate("/events")}>
            Start event
          </button>
          <button type="button" className="button-ghost" onClick={() => navigate("/tournaments")}>
            Open tournament map
          </button>
          <button type="button" className="button-ghost" onClick={() => navigate("/replay")}>
            Review latest replay
          </button>
        </div>
      </section>

      <section className="panel">
        <div className="panel-header">
          <h2>Recent Games</h2>
          <span>{recentGames.length} latest finishes</span>
        </div>
        {recentGames.length === 0 ? (
          <EmptyState>No finished games yet. Start an event and activity will show up here.</EmptyState>
        ) : (
          <div className="table">
            {recentGames.map((game) => (
              <button
                type="button"
                className="table-row replay-row"
                key={game.id}
                onClick={() => navigate(`/replay?gameId=${encodeURIComponent(game.id)}`)}
              >
                <div>
                  <strong>
                    White: {participantName(game.white_participant, "White")} • Black:{" "}
                    {participantName(game.black_participant, "Black")}
                  </strong>
                  <p>
                    {matchResultText(game.result)} • {formatLabel(game.termination)}
                  </p>
                </div>
                <div className="chip">{new Date(game.completed_at).toLocaleTimeString()}</div>
              </button>
            ))}
          </div>
        )}
      </section>

      <section className="panel">
        <div className="panel-header">
          <h2>Standings</h2>
          <div className="panel-header-actions">
            <span>All engines</span>
            <button type="button" className="button-ghost compact-button" onClick={() => navigate("/tournaments")}>
              Tournament map
            </button>
          </div>
        </div>
        <p className="panel-copy">Ratings update as finished games roll in.</p>
        {(leaderboard.data?.length ?? 0) === 0 ? (
          <EmptyState>No standings yet.</EmptyState>
        ) : (
          <div className="leaderboard">
            {leaderboard.data?.map((entry, index) => (
              <div className="leader-row" key={entry.participant.id}>
                <div className="leader-rank">{index + 1}</div>
                <div>
                  <strong>{participantName(entry.participant, entry.participant.id)}</strong>
                  <p>
                    {entry.games_played} games • {entry.wins}W {entry.draws}D {entry.losses}L
                  </p>
                </div>
                <div className="leader-rating">{entry.rating.toFixed(1)}</div>
              </div>
            ))}
          </div>
        )}
      </section>
    </>
  );
}
