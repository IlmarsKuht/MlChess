import type { GameRecord, MatchSeries, Tournament } from "../../app/types";
import { formatLabel, formatTournamentKind, matchResultText, roundLabel, statusTone, winnerText } from "../../shared/lib/format";
import { participantName } from "../../shared/lib/participants";
import { StatusBadge } from "../../shared/ui";

export function TournamentMapCard({
  tournament,
  matches,
  gameByMatchId,
  poolNameById,
  onWatch
}: {
  tournament: Tournament;
  matches: MatchSeries[];
  gameByMatchId: Record<string, GameRecord>;
  poolNameById: Record<string, string>;
  onWatch: (matchId: string) => void;
}) {
  const rounds = new Map<number, MatchSeries[]>();
  for (const match of [...matches].sort((left, right) => left.round_index - right.round_index || left.game_index - right.game_index)) {
    const current = rounds.get(match.round_index) ?? [];
    current.push(match);
    rounds.set(match.round_index, current);
  }

  const completedCount = matches.filter((match) => match.status === "completed").length;

  return (
    <div className="tournament-card">
      <div className="tournament-card-header">
        <div>
          <strong>{tournament.name}</strong>
          <p>
            {formatTournamentKind(tournament.kind)} • {poolNameById[tournament.pool_id] ?? "Unknown format"} •{" "}
            {tournament.games_per_pairing} game{tournament.games_per_pairing === 1 ? "" : "s"} per pairing
          </p>
          <p>
            {tournament.participant_version_ids.length} participants • {completedCount}/{matches.length} matches
            finished
          </p>
        </div>
        <StatusBadge tone={statusTone(tournament.status)}>{formatLabel(tournament.status)}</StatusBadge>
      </div>

      <div className="tournament-rounds">
        {[...rounds.entries()].map(([roundIndex, roundMatches]) => (
          <div className="tournament-round" key={roundIndex}>
            <div className="section-heading">{roundLabel(tournament.kind, roundIndex)}</div>
            <div className="tournament-round-list">
              {roundMatches.map((match) => {
                const game = gameByMatchId[match.id];
                return (
                  <div className="match-card" key={match.id}>
                    <div className="match-card-header">
                      <StatusBadge tone={statusTone(match.status)}>{formatLabel(match.status)}</StatusBadge>
                      <span className="subtle">{matchResultText(game?.result)}</span>
                    </div>
                    <div className="match-card-sides">
                      <div>
                        <span>White</span>
                        <strong>{participantName(match.white_participant, "White")}</strong>
                      </div>
                      <div>
                        <span>Black</span>
                        <strong>{participantName(match.black_participant, "Black")}</strong>
                      </div>
                    </div>
                    <p className="match-card-summary">
                      {winnerText(game?.result)}
                      {game ? ` • ${formatLabel(game.termination)}` : ""}
                    </p>
                    {match.watch_state === "live" ? (
                      <button type="button" className="button-ghost compact-button" onClick={() => onWatch(match.id)}>
                        Watch
                      </button>
                    ) : null}
                  </div>
                );
              })}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
