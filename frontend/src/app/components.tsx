import { ReactNode } from "react";

import { pieceImages } from "./pieces";
import type {
  BoardMoveMarker,
  GameRecord,
  MatchSeries,
  Tournament
} from "./types";
import {
  boardIndexToSquare,
  formatLabel,
  formatTournamentKind,
  groupedMoveRows,
  matchResultText,
  participantName,
  roundLabel,
  statusTone,
  winnerText
} from "./utils";

export function Field({
  label,
  hint,
  children
}: {
  label: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <div className="field">
      <div className="field-header">
        <span className="field-label">{label}</span>
        {hint ? <span className="field-hint">{hint}</span> : null}
      </div>
      {children}
    </div>
  );
}

export function MetricCard({ label, value }: { label: string; value: string }) {
  return (
    <div className="metric">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

export function StatusBadge({
  tone,
  children
}: {
  tone: "quiet" | "running" | "good" | "warning";
  children: ReactNode;
}) {
  return <span className={`status-badge status-${tone}`}>{children}</span>;
}

export function EmptyState({ children }: { children: ReactNode }) {
  return <div className="empty-state">{children}</div>;
}

export function EngineDocumentation({ text }: { text: string }) {
  const blocks = text
    .trim()
    .split(/\n\s*\n/)
    .map((block) => block.trim())
    .filter(Boolean);

  return (
    <div className="engine-doc">
      {blocks.map((block, index) => {
        const lines = block.split("\n").map((line) => line.trim()).filter(Boolean);
        if (lines.length === 0) {
          return null;
        }

        if (lines.length === 1 && lines[0].startsWith("## ")) {
          return (
            <h3 className="engine-doc-heading" key={index}>
              {lines[0].slice(3)}
            </h3>
          );
        }

        if (lines.length === 1 && lines[0].startsWith("### ")) {
          return (
            <h4 className="engine-doc-subheading" key={index}>
              {lines[0].slice(4)}
            </h4>
          );
        }

        if (lines.every((line) => line.startsWith("- "))) {
          return (
            <ul className="engine-doc-list" key={index}>
              {lines.map((line) => (
                <li key={line}>{line.slice(2)}</li>
              ))}
            </ul>
          );
        }

        return (
          <p className="engine-doc-paragraph" key={index}>
            {lines.join(" ")}
          </p>
        );
      })}
    </div>
  );
}

export function BoardView({
  squares,
  selectedSquare,
  legalMoveMarkers = [],
  invalidSquare,
  interactive = false,
  hoverableSquares,
  onSquareClick,
  orientation = "white"
}: {
  squares: string[];
  selectedSquare?: string;
  legalMoveMarkers?: BoardMoveMarker[];
  invalidSquare?: string;
  interactive?: boolean;
  hoverableSquares?: Set<string>;
  onSquareClick?: (index: number) => void;
  orientation?: "white" | "black";
}) {
  const markerBySquare = new Map(legalMoveMarkers.map((marker) => [marker.square, marker.kind]));
  return (
    <div className="board-frame">
      <div className="board">
        {squares.map((piece, index) => {
          const square = boardIndexToSquare(index, orientation);
          const markerKind = markerBySquare.get(square);
          const hoverable = hoverableSquares?.has(square) ?? false;
          return (
            <button
              key={`${piece}-${index}`}
              type="button"
              className={`square ${(Math.floor(index / 8) + index) % 2 === 0 ? "light" : "dark"} ${
                selectedSquare === square ? "square-selected" : ""
              } ${invalidSquare === square ? "square-invalid" : ""} ${interactive ? "square-interactive" : ""} ${
                piece ? "square-has-piece" : ""
              } ${hoverable ? "square-hoverable-piece" : ""}`}
              onClick={() => onSquareClick?.(index)}
              disabled={!interactive}
            >
              {markerKind ? (
                <span
                  className={`square-marker ${
                    markerKind === "capture" ? "square-marker-capture" : "square-marker-quiet"
                  }`}
                  aria-hidden="true"
                />
              ) : null}
              {piece ? <img className="piece-image" src={pieceImages[piece]} alt="" draggable={false} /> : null}
            </button>
          );
        })}
      </div>
    </div>
  );
}

export function EngineSideCard({
  side,
  title,
  name,
  clock,
  active = false
}: {
  side: "white" | "black";
  title: string;
  name: string;
  clock?: string;
  active?: boolean;
}) {
  return (
    <div className={`engine-card engine-card-${side} ${active ? "engine-card-active" : ""}`}>
      <span>{title}</span>
      <strong>{name}</strong>
      {clock ? <p>{clock}</p> : null}
    </div>
  );
}

export function StatCard({ label, value }: { label: string; value: string }) {
  return (
    <div className="stat-card">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

export function MoveList({ moves, activePly }: { moves: string[]; activePly: number }) {
  const rows = groupedMoveRows(moves);

  if (rows.length === 0) {
    return <EmptyState>No moves revealed yet.</EmptyState>;
  }

  return (
    <div className="move-list">
      {rows.map((row) => {
        const whitePly = (row.index - 1) * 2 + 1;
        const blackPly = whitePly + 1;

        return (
          <div className="move-row" key={row.index}>
            <span className="move-index">{row.index}.</span>
            <span className={activePly === whitePly ? "move-active" : ""}>{row.white}</span>
            <span className={activePly === blackPly ? "move-active" : ""}>{row.black ?? ""}</span>
          </div>
        );
      })}
    </div>
  );
}

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
                    {match.status === "running" ? (
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
