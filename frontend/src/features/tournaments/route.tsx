import { useNavigate } from "react-router-dom";

import { useGamesQuery, useMatchesQuery, usePoolsQuery, useTournamentsQuery } from "../../shared/queries/arena";
import { EmptyState, RouteErrorState, RouteLoadingState } from "../../shared/ui";
import { TournamentMapCard } from "./TournamentMapCard";

export function TournamentsPage() {
  const navigate = useNavigate();
  const tournaments = useTournamentsQuery(3000);
  const matches = useMatchesQuery(3000);
  const games = useGamesQuery(3000);
  const pools = usePoolsQuery();

  if (tournaments.isLoading || matches.isLoading || games.isLoading || pools.isLoading) {
    return <RouteLoadingState message="Loading tournament map..." />;
  }

  const error = tournaments.error ?? matches.error ?? games.error ?? pools.error;
  if (error) {
    return <RouteErrorState message={error.message} />;
  }

  const sortedTournaments = [...(tournaments.data ?? [])].sort((left, right) => {
    const leftRunning = left.status === "running" ? 1 : 0;
    const rightRunning = right.status === "running" ? 1 : 0;
    if (leftRunning !== rightRunning) {
      return rightRunning - leftRunning;
    }
    return left.name.localeCompare(right.name);
  });
  const poolNameById = Object.fromEntries((pools.data ?? []).map((pool) => [pool.id, pool.name]));
  const gameByMatchId = Object.fromEntries((games.data ?? []).map((game) => [game.match_id, game]));

  return (
    <section className="panel tournament-panel">
      <div className="panel-header">
        <h2>Tournament Map</h2>
        <span>{sortedTournaments.length} tournament{sortedTournaments.length === 1 ? "" : "s"}</span>
      </div>
      <p className="panel-copy">
        Follow who played whom, which round each match belongs to, and which side won without needing a true
        elimination bracket.
      </p>

      {sortedTournaments.length === 0 ? (
        <EmptyState>No tournaments have been created yet.</EmptyState>
      ) : (
        <div className="tournament-stack">
          {sortedTournaments.map((tournament) => (
            <TournamentMapCard
              key={tournament.id}
              tournament={tournament}
              matches={(matches.data ?? []).filter((match) => match.tournament_id === tournament.id)}
              gameByMatchId={gameByMatchId}
              poolNameById={poolNameById}
              onWatch={(matchId) => navigate(`/watch/${encodeURIComponent(matchId)}`)}
            />
          ))}
        </div>
      )}
    </section>
  );
}
