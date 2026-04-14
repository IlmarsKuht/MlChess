import { useMemo, useState } from "react";
import { useSearchParams } from "react-router-dom";

import { buildReplayFrames, fenToBoard } from "../../shared/chess/board";
import { formatLabel, matchResultText } from "../../shared/lib/format";
import { participantName } from "../../shared/lib/participants";
import { useGamesQuery } from "../../shared/queries/arena";
import { BoardView, EmptyState, Field, RouteErrorState, RouteLoadingState } from "../../shared/ui";
import { useReplayQuery } from "./api";

export function ReplayPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const [selectedPly, setSelectedPly] = useState(0);
  const gameId = searchParams.get("gameId") ?? "";
  const games = useGamesQuery();
  const replay = useReplayQuery(gameId);

  const replayFrames = useMemo(() => buildReplayFrames(replay.data ?? null), [replay.data]);
  const currentFen = replayFrames[Math.min(selectedPly, Math.max(replayFrames.length - 1, 0))];
  const boardSquares = currentFen ? fenToBoard(currentFen) : [];

  if (games.isLoading) {
    return <RouteLoadingState message="Loading replay library..." />;
  }
  if (games.error) {
    return <RouteErrorState message={games.error.message} />;
  }
  if (replay.error) {
    return <RouteErrorState message={replay.error.message} />;
  }

  return (
    <section className="panel replay-panel">
      <div className="panel-header">
        <h2>Replay</h2>
        <span>{gameId ? "Game selected" : "Pick a game"}</span>
      </div>
      <div className="table games-feed">
        {(games.data ?? []).slice(0, 8).map((game) => (
          <button
            type="button"
            className={`table-row replay-row ${gameId === game.id ? "table-row-active" : ""}`}
            key={game.id}
            onClick={() => {
              setSelectedPly(0);
              setSearchParams(game.id ? { gameId: game.id } : {});
            }}
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
            <div className="chip">{new Date(game.completed_at).toLocaleString()}</div>
          </button>
        ))}
      </div>

      {(games.data?.length ?? 0) === 0 && (
        <EmptyState>No games have finished yet. Start an event and this area will fill in automatically.</EmptyState>
      )}

      {!replay.data && (games.data?.length ?? 0) > 0 && (
        <EmptyState>Choose a finished game above to load the board, result, and move list.</EmptyState>
      )}

      {replay.data && (
        <div className="replay-content">
          {replay.data.variant === "standard" && boardSquares.length > 0 ? (
            <div className="board-stage">
              <BoardView squares={boardSquares} />
              <div className="scrubber-row">
                <span>
                  Ply {selectedPly} / {Math.max(replayFrames.length - 1, 0)}
                </span>
                <input
                  type="range"
                  min={0}
                  max={Math.max(replayFrames.length - 1, 0)}
                  value={selectedPly}
                  onChange={(event) => setSelectedPly(Number(event.target.value))}
                />
              </div>
            </div>
          ) : (
            <EmptyState>
              Board replay is shown for standard games. Chess960 games still include move and result details.
            </EmptyState>
          )}

          <div className="replay-meta">
            <div className="result-strip">
              <strong>Result</strong>
              <span>
                {matchResultText(replay.data.result)} via {formatLabel(replay.data.termination)}
              </span>
            </div>
            <Field label="Move record">
              <textarea className="replay-textarea" readOnly rows={8} value={replay.data.pgn} />
            </Field>
          </div>
        </div>
      )}
    </section>
  );
}
