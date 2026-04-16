import { useEffect, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";

import { setUiDebugState } from "../../app/debug";
import type { BoardMoveMarker } from "../../app/types";
import {
  buildReplayFrames,
  boardIndexToSquare,
  fenToBoard,
  legalMovesByOrigin,
  maybePromotion,
  orientSquares,
  squareName
} from "../../shared/chess/board";
import {
  formatClock,
  outcomeHeadline,
  outcomeSubtitle,
  formatLabel,
  formatRelativeTime,
  matchResultText,
  roundLabel,
  statusTone,
  winnerText
} from "../../shared/lib/format";
import { loadErrorMessage } from "../../shared/lib/errors";
import { participantName } from "../../shared/lib/participants";
import { useGamesQuery, useMatchesQuery, usePoolsQuery, useTournamentsQuery } from "../../shared/queries/arena";
import { BoardView, EmptyState, EngineSideCard, MoveList, StatCard, StatusBadge } from "../../shared/ui";
import { DebugDrawer } from "../debug/DebugDrawer";
import { useReplayQuery } from "../replay/api";
import { useConfirmedLiveMatch } from "./live";
import { useLivePlayback } from "./livePlayback";
import { isPendingLiveWatchMatch, isTerminalLiveStatus, lastWatchedKey, liveClockElapsedMs } from "./model";

export function WatchPage() {
  const navigate = useNavigate();
  const { matchId = "" } = useParams();
  const matches = useMatchesQuery(3000);
  const pools = usePoolsQuery();
  const tournaments = useTournamentsQuery(3000);
  const games = useGamesQuery();
  const [selectedPly, setSelectedPly] = useState(0);
  const [selectedBoardSquare, setSelectedBoardSquare] = useState("");
  const [invalidBoardSquare, setInvalidBoardSquare] = useState("");
  const [isSubmittingHumanMove, setIsSubmittingHumanMove] = useState(false);
  const [liveNowMs, setLiveNowMs] = useState(() => Date.now());
  const [error, setError] = useState("");
  const [boardEventFlash, setBoardEventFlash] = useState(false);
  const [latestMoveFlash, setLatestMoveFlash] = useState(false);
  const [resultReveal, setResultReveal] = useState(false);

  const selectedLiveMatch = (matches.data ?? []).find((match) => match.id === matchId) ?? null;
  const selectedWatchGame =
    selectedLiveMatch?.watch_state === "replay"
      ? (games.data ?? []).find((game) => game.id === selectedLiveMatch.game_id) ??
        (games.data ?? []).find((game) => game.match_id === selectedLiveMatch.id) ??
        null
      : null;
  const watchReplayQuery = useReplayQuery(selectedWatchGame?.id ?? "");
  const selectedWatchReplay = watchReplayQuery.data ?? (selectedWatchGame
    ? {
        id: selectedWatchGame.id,
        variant: selectedWatchGame.variant,
        start_fen: selectedWatchGame.start_fen,
        frames: [],
        pgn: selectedWatchGame.pgn,
        moves_uci: selectedWatchGame.moves_uci,
        result: selectedWatchGame.result,
        termination: selectedWatchGame.termination
      }
    : null);

  const shouldKeepConfirmedLiveMatch =
    !!selectedLiveMatch &&
    (selectedLiveMatch.watch_state === "live" ||
      (selectedLiveMatch.watch_state === "replay" && !selectedWatchReplay));
  const confirmedLiveMatch = useConfirmedLiveMatch(shouldKeepConfirmedLiveMatch ? matchId : "");
  const confirmedLiveSnapshot = confirmedLiveMatch.snapshot;
  const poolById = Object.fromEntries((pools.data ?? []).map((pool) => [pool.id, pool]));
  const poolNameById = Object.fromEntries((pools.data ?? []).map((pool) => [pool.id, pool.name]));
  const tournamentById = Object.fromEntries((tournaments.data ?? []).map((tournament) => [tournament.id, tournament]));
  const liveVariant = selectedLiveMatch ? poolById[selectedLiveMatch.pool_id]?.variant ?? "standard" : "standard";
  const rawLiveGame =
    confirmedLiveSnapshot && selectedLiveMatch
      ? {
          match_id: confirmedLiveSnapshot.match_id,
          tournament_id: selectedLiveMatch.tournament_id,
          pool_id: selectedLiveMatch.pool_id,
          variant: liveVariant,
          start_fen: confirmedLiveSnapshot.start_fen,
          current_fen: confirmedLiveSnapshot.fen,
          moves_uci: confirmedLiveSnapshot.moves,
          white_time_left_ms: confirmedLiveSnapshot.white_remaining_ms,
          black_time_left_ms: confirmedLiveSnapshot.black_remaining_ms,
          status: confirmedLiveSnapshot.status,
          result: confirmedLiveSnapshot.result === "none" ? null : confirmedLiveSnapshot.result,
          termination: confirmedLiveSnapshot.termination === "none" ? null : confirmedLiveSnapshot.termination,
          updated_at: new Date(confirmedLiveSnapshot.server_now_unix_ms).toISOString(),
          live_frames: confirmedLiveMatch.timeline.map((frame) => ({
            ply: frame.moves.length,
            fen: frame.fen,
            move_uci: frame.move_uci,
            white_time_left_ms: frame.white_time_left_ms,
            black_time_left_ms: frame.black_time_left_ms,
            updated_at: new Date(frame.server_now_unix_ms).toISOString(),
            turn_started_server_unix_ms: frame.turn_started_server_unix_ms,
            side_to_move: frame.side_to_move === "black" ? "black" : "white",
            status: frame.status,
            result: frame.result === "none" ? null : frame.result,
            termination: frame.termination === "none" ? null : frame.termination
          })),
          white_participant: selectedLiveMatch.white_participant,
          black_participant: selectedLiveMatch.black_participant,
          interactive: selectedLiveMatch.interactive,
          human_turn:
            selectedLiveMatch.interactive &&
            ((selectedLiveMatch.white_participant.kind === "human_player" && confirmedLiveSnapshot.side_to_move === "white") ||
              (selectedLiveMatch.black_participant.kind === "human_player" && confirmedLiveSnapshot.side_to_move === "black"))
        }
      : null;
  const livePlayback = useLivePlayback({
    matchId: rawLiveGame?.match_id ?? "",
    liveFrameCount: rawLiveGame?.live_frames.length ?? 0,
    interactive: rawLiveGame?.interactive ?? false,
    activelyWatching: true
  });

  useEffect(() => {
    if (!matchId) {
      return;
    }
    try {
      window.localStorage.setItem(lastWatchedKey, matchId);
    } catch {
      // Ignore storage failures.
    }
  }, [matchId]);

  useEffect(() => {
    if (confirmedLiveMatch.error) {
      setError(confirmedLiveMatch.error);
    }
  }, [confirmedLiveMatch.error]);

  useEffect(() => {
    setUiDebugState({
      route: `${window.location.pathname}${window.location.search}${window.location.hash || "#/"}`,
      selected_match_id: matchId || undefined,
      selected_tournament_id: selectedLiveMatch?.tournament_id ?? rawLiveGame?.tournament_id,
      selected_game_id: selectedWatchReplay?.id,
      current_snapshot_seq: confirmedLiveSnapshot?.seq,
      current_live_status: confirmedLiveSnapshot?.status,
      live_summary: confirmedLiveSnapshot
        ? `seq ${confirmedLiveSnapshot.seq} ${confirmedLiveSnapshot.status} ${confirmedLiveSnapshot.side_to_move}`
        : rawLiveGame
          ? `${rawLiveGame.status} ${rawLiveGame.moves_uci.length} moves`
          : undefined,
      last_ui_error: error || undefined
    });
  }, [
    matchId,
    selectedLiveMatch?.tournament_id,
    rawLiveGame?.tournament_id,
    selectedWatchReplay?.id,
    confirmedLiveSnapshot?.seq,
    confirmedLiveSnapshot?.status,
    confirmedLiveSnapshot?.side_to_move,
    rawLiveGame?.status,
    rawLiveGame?.moves_uci.length,
    error
  ]);

  useEffect(() => {
    if (selectedWatchReplay) {
      const replayFrames = buildReplayFrames(selectedWatchReplay);
      setSelectedPly(Math.max(replayFrames.length - 1, 0));
    }
  }, [selectedWatchReplay?.id]);

  useEffect(() => {
    if (!rawLiveGame || rawLiveGame.status !== "running") {
      return;
    }

    const timer = window.setInterval(() => {
      setLiveNowMs(Date.now());
    }, 250);

    return () => window.clearInterval(timer);
  }, [rawLiveGame?.match_id, rawLiveGame?.status]);

  useEffect(() => {
    setSelectedBoardSquare("");
    setInvalidBoardSquare("");
  }, [rawLiveGame?.match_id]);

  useEffect(() => {
    if (!invalidBoardSquare) {
      return;
    }
    const timer = window.setTimeout(() => setInvalidBoardSquare(""), 280);
    return () => window.clearTimeout(timer);
  }, [invalidBoardSquare]);

  useEffect(() => {
    if ((rawLiveGame?.live_frames.length ?? 0) === 0) {
      return;
    }
    setBoardEventFlash(true);
    setLatestMoveFlash(true);
    const timer = window.setTimeout(() => {
      setBoardEventFlash(false);
      setLatestMoveFlash(false);
    }, 900);
    return () => window.clearTimeout(timer);
  }, [rawLiveGame?.live_frames.length]);

  const watchReplayFrames = buildReplayFrames(selectedWatchReplay);
  const watchReplayFen = watchReplayFrames[Math.min(selectedPly, Math.max(watchReplayFrames.length - 1, 0))] ?? "";
  const watchReplaySquares = watchReplayFen ? fenToBoard(watchReplayFen) : [];
  const allLiveFrames = rawLiveGame?.live_frames ?? [];
  const displayedLiveFrameCount = livePlayback.displayedLiveFrameCount;
  const selectedLivePly = livePlayback.selectedLivePly;
  const isLiveFollowing = livePlayback.isLiveFollowing;
  const revealedLiveFrames = allLiveFrames.slice(0, displayedLiveFrameCount);
  const maxDisplayedLiveFrameIndex = Math.max(revealedLiveFrames.length - 1, 0);
  const visibleLiveFrameIndex = isLiveFollowing
    ? maxDisplayedLiveFrameIndex
    : Math.min(selectedLivePly, maxDisplayedLiveFrameIndex);
  const visibleLiveFrame = revealedLiveFrames[Math.min(visibleLiveFrameIndex, maxDisplayedLiveFrameIndex)] ?? null;
  const visibleLivePly = visibleLiveFrame?.ply ?? 0;
  const liveFen = visibleLiveFrame?.fen ?? "";
  const liveBoardSquares = liveFen ? fenToBoard(liveFen) : [];
  const displayedLiveMoves = revealedLiveFrames.flatMap((frame) => (frame.move_uci ? [frame.move_uci] : []));
  const visibleLiveUpdatedAtMs = visibleLiveFrame ? new Date(visibleLiveFrame.updated_at).getTime() : 0;
  const runningClockElapsedMs = liveClockElapsedMs({
    status: visibleLiveFrame?.status,
    isLiveFollowing,
    liveNowMs,
    turnStartedServerUnixMs: visibleLiveFrame?.turn_started_server_unix_ms
  });
  const displayedWhiteClockMs =
    visibleLiveFrame && visibleLiveFrame.side_to_move === "white"
      ? Math.max(0, visibleLiveFrame.white_time_left_ms - runningClockElapsedMs)
      : visibleLiveFrame?.white_time_left_ms ?? 0;
  const displayedBlackClockMs =
    visibleLiveFrame && visibleLiveFrame.side_to_move === "black"
      ? Math.max(0, visibleLiveFrame.black_time_left_ms - runningClockElapsedMs)
      : visibleLiveFrame?.black_time_left_ms ?? 0;
  const visibleLiveStatus = visibleLiveFrame?.status ?? "";
  const visibleLiveResult = visibleLiveFrame?.result ?? null;
  const visibleLiveTermination = visibleLiveFrame?.termination ?? null;
  const terminalVisibleLive = isTerminalLiveStatus(visibleLiveStatus);
  const liveSideToMove = visibleLiveFrame?.side_to_move ?? "white";
  const liveWhiteParticipant = rawLiveGame?.white_participant ?? selectedLiveMatch?.white_participant ?? null;
  const liveBlackParticipant = rawLiveGame?.black_participant ?? selectedLiveMatch?.black_participant ?? null;
  const interactiveLive = rawLiveGame?.interactive ?? selectedLiveMatch?.interactive ?? false;
  const liveBoardOrientation = interactiveLive && liveBlackParticipant?.kind === "human_player" ? "black" : "white";
  const orientedLiveBoardSquares = orientSquares(liveBoardSquares, liveBoardOrientation);
  const standardMoveHints = liveVariant === "standard";
  const legalMovesForCurrentPosition =
    rawLiveGame && standardMoveHints ? legalMovesByOrigin(rawLiveGame.current_fen) : new Map<string, BoardMoveMarker[]>();
  const selectedSquareMarkers = selectedBoardSquare ? legalMovesForCurrentPosition.get(selectedBoardSquare) ?? [] : [];
  const selectableSquares =
    interactiveLive && rawLiveGame?.human_turn
      ? standardMoveHints
        ? new Set(legalMovesForCurrentPosition.keys())
        : selectableHumanPieceSquares(liveBoardSquares, liveSideToMove)
      : new Set<string>();
  const selectedLiveTournament = selectedLiveMatch ? tournamentById[selectedLiveMatch.tournament_id] : undefined;
  const pendingSelectedLiveMatch = selectedLiveMatch !== null && isPendingLiveWatchMatch(selectedLiveMatch);
  const pendingLiveMatch =
    ((selectedLiveMatch?.watch_state === "live" || pendingSelectedLiveMatch) && !rawLiveGame) ||
    (!selectedLiveMatch && !rawLiveGame && !!matchId);

  const visibleWinnerSide =
    visibleLiveResult === "white_win"
      ? "white"
      : visibleLiveResult === "black_win"
        ? "black"
        : null;
  const replayWinnerSide =
    selectedWatchReplay?.result === "white_win" ? "white" : selectedWatchReplay?.result === "black_win" ? "black" : null;
  const whiteUrgency = urgencyForClock(displayedWhiteClockMs, liveSideToMove === "white");
  const blackUrgency = urgencyForClock(displayedBlackClockMs, liveSideToMove === "black");
  const visibleLatestPly = displayedLiveMoves.length;
  const replayLatestPly = selectedWatchReplay?.moves_uci.length ?? 0;

  useEffect(() => {
    if (!terminalVisibleLive) {
      return;
    }
    setResultReveal(true);
    const timer = window.setTimeout(() => setResultReveal(false), 1400);
    return () => window.clearTimeout(timer);
  }, [terminalVisibleLive, visibleLiveResult, visibleLiveTermination]);

  function liveStatusMessage() {
    if (terminalVisibleLive) {
      return "Final move played. Replay details are loading below.";
    }
    if (interactiveLive) {
      if (rawLiveGame?.human_turn) {
        return isSubmittingHumanMove ? "Submitting your move." : "Your move. Click a piece, then its destination.";
      }
      if (liveSideToMove === "white" || liveSideToMove === "black") {
        return "Engine thinking. Stay ready for the reply.";
      }
      return "Engine thinking.";
    }
    const activeUrgency = liveSideToMove === "white" ? whiteUrgency : blackUrgency;
    if (activeUrgency === "critical") {
      return `${formatLabel(liveSideToMove)} under time pressure.`;
    }
    if (liveSideToMove === "white" || liveSideToMove === "black") {
      return `${formatLabel(liveSideToMove)} thinking.`;
    }
    return confirmedLiveMatch.isConnected ? "Live board updating." : "Reconnecting live feed.";
  }

  const reviewReplayHref = selectedLiveMatch?.game_id ? `/replay?gameId=${encodeURIComponent(selectedLiveMatch.game_id)}` : "";

  async function submitHumanMove(uci: string) {
    if (!rawLiveGame) {
      return;
    }
    setIsSubmittingHumanMove(true);
    setError("");
    try {
      await confirmedLiveMatch.submitMove(uci);
      setSelectedBoardSquare("");
    } catch (moveError) {
      setError(loadErrorMessage(moveError));
    } finally {
      setIsSubmittingHumanMove(false);
    }
  }

  function handleBoardSquareClick(index: number) {
    if (!rawLiveGame || !interactiveLive || !rawLiveGame.human_turn || isSubmittingHumanMove) {
      return;
    }
    const square = boardIndexToSquare(index, liveBoardOrientation);
    const piece = liveBoardSquares[liveBoardOrientation === "white" ? index : 63 - index];
    const selectable = selectableSquares.has(square);

    if (!selectedBoardSquare) {
      if (selectable) {
        setSelectedBoardSquare(square);
        setError("");
      } else if (piece) {
        setInvalidBoardSquare(square);
      }
      return;
    }
    if (selectedBoardSquare === square) {
      setSelectedBoardSquare("");
      return;
    }
    if (selectable) {
      setSelectedBoardSquare(square);
      setError("");
      return;
    }
    const legalDestination = selectedSquareMarkers.find((marker) => marker.square === square);
    if (!legalDestination && standardMoveHints) {
      setInvalidBoardSquare(square);
      return;
    }
    const fromIndex = liveBoardSquares.findIndex((_, boardIndex) => squareName(boardIndex) === selectedBoardSquare);
    const fromPiece = fromIndex >= 0 ? liveBoardSquares[fromIndex] : "";
    void submitHumanMove(`${selectedBoardSquare}${square}${maybePromotion(selectedBoardSquare, square, fromPiece)}`);
  }

  const watchTitle = selectedLiveMatch?.interactive
    ? "Fullscreen human match"
    : selectedLiveMatch?.watch_state === "replay"
      ? "Fullscreen match replay"
      : "Fullscreen engine viewer";
  const watchCopy = selectedLiveMatch?.interactive
    ? "Play directly on the board, watch the engine answer, and keep the clocks and move list in view."
    : selectedLiveMatch?.watch_state === "replay"
      ? "Review a finished match with the same spacious board and side panels used for live viewing."
      : "Follow one match at a readable pace with clear White and Black panels and a slightly delayed move feed.";

  return (
    <div className="watch-shell">
      <header className="watch-header">
        <div className="watch-header-copy">
          <p className="eyebrow">{selectedLiveMatch?.interactive ? "Play vs Engine" : "Live Watch"}</p>
          <h1>{watchTitle}</h1>
          <p className="lede">{watchCopy}</p>
        </div>
        <div className="watch-header-actions">
          <button
            type="button"
            className="button-ghost"
            onClick={() => navigate(selectedLiveMatch?.interactive ? "/play-engine" : "/live-duel")}
          >
            Back to arena
          </button>
          {selectedLiveMatch ? (
            <StatusBadge tone={statusTone(selectedLiveMatch.status)}>
              {selectedLiveMatch.interactive
                ? "Human game"
                : roundLabel(selectedLiveTournament?.kind ?? "round_robin", selectedLiveMatch.round_index)}
            </StatusBadge>
          ) : null}
        </div>
      </header>

      <DebugDrawer />

      {error && <section className="banner banner-error">{error}</section>}

      {pendingLiveMatch ? (
        <section className="panel watch-panel">
          <EmptyState>
            Preparing the live board. The match exists, and the viewer is waiting for the first live state to arrive.
          </EmptyState>
        </section>
      ) : selectedLiveMatch?.watch_state === "replay" && selectedWatchReplay ? (
        <section className="panel watch-panel">
          <div className="watch-stage">
            <div className="watch-board-column">
              <div className="watch-meta-bar">
                <div>
                  <strong>
                    {participantName(selectedLiveMatch.white_participant, "White")} vs{" "}
                    {participantName(selectedLiveMatch.black_participant, "Black")}
                  </strong>
                  <p>
                    {poolNameById[selectedLiveMatch.pool_id] ?? "Unknown format"} •{" "}
                    {selectedLiveMatch.interactive
                      ? "Human game"
                      : roundLabel(selectedLiveTournament?.kind ?? "round_robin", selectedLiveMatch.round_index)}
                  </p>
                </div>
                <StatusBadge tone={statusTone(selectedLiveMatch.status)}>{formatLabel(selectedLiveMatch.status)}</StatusBadge>
              </div>

              {selectedWatchReplay.result ? (
                <section
                  className={`watch-outcome-hero watch-outcome-${selectedWatchReplay.result} ${resultReveal ? "watch-outcome-reveal" : ""}`}
                >
                  <div>
                    <p className="eyebrow">Final Result</p>
                    <h2>{outcomeHeadline(selectedWatchReplay.result)}</h2>
                    <p className="watch-outcome-copy">
                      {outcomeSubtitle(selectedWatchReplay.result, selectedWatchReplay.termination)}
                    </p>
                  </div>
                  <div className="watch-outcome-actions">
                    <button type="button" className="button-ghost" onClick={() => navigate("/replay")}>
                      Review replay
                    </button>
                    <button
                      type="button"
                      className="button-ghost"
                      onClick={() => navigate(selectedLiveMatch?.interactive ? "/play-engine" : "/live-duel")}
                    >
                      {selectedLiveMatch?.interactive ? "Play again" : "Back to arena"}
                    </button>
                  </div>
                </section>
              ) : null}

              {watchReplaySquares.length > 0 ? (
                <div
                  className={`watch-board-wrap ${
                    replayWinnerSide ? `watch-board-wrap-winner-${replayWinnerSide}` : selectedWatchReplay.result === "draw" ? "watch-board-wrap-draw" : ""
                  }`}
                >
                  <BoardView squares={watchReplaySquares} />
                </div>
              ) : (
                <EmptyState>Board replay is unavailable for this game.</EmptyState>
              )}

              <div className="watch-controls">
                <div className="scrubber-row">
                  <span>
                    Ply {selectedPly} / {Math.max(watchReplayFrames.length - 1, 0)}
                  </span>
                  <input
                    type="range"
                    min={0}
                    max={Math.max(watchReplayFrames.length - 1, 0)}
                    value={Math.min(selectedPly, Math.max(watchReplayFrames.length - 1, 0))}
                    onChange={(event) => setSelectedPly(Number(event.target.value))}
                  />
                </div>
              </div>
            </div>

            <div className="watch-info-column">
              <div className="watch-side-grid">
                <EngineSideCard
                  side="white"
                  title={selectedLiveMatch.white_participant.kind === "human_player" ? "You" : "White engine"}
                  name={participantName(selectedLiveMatch.white_participant, "White")}
                  clock={formatClock(selectedWatchGame?.white_time_left_ms ?? 0)}
                  winner={replayWinnerSide === "white"}
                />
                <EngineSideCard
                  side="black"
                  title={selectedLiveMatch.black_participant.kind === "human_player" ? "You" : "Black engine"}
                  name={participantName(selectedLiveMatch.black_participant, "Black")}
                  clock={formatClock(selectedWatchGame?.black_time_left_ms ?? 0)}
                  winner={replayWinnerSide === "black"}
                />
              </div>

              <div className="watch-stats-grid">
                <StatCard label="Visible plies" value={String(selectedWatchReplay.moves_uci.length)} />
                <StatCard
                  label="Updated"
                  value={selectedWatchGame ? formatRelativeTime(new Date(selectedWatchGame.completed_at).getTime()) : "--"}
                />
                <StatCard label="Result" value={matchResultText(selectedWatchReplay.result)} />
              </div>

              <div className="result-strip">
                <strong>Replay summary</strong>
                <span>
                  {winnerText(selectedWatchReplay.result)}
                  {selectedWatchReplay.termination ? ` via ${formatLabel(selectedWatchReplay.termination)}` : ""}
                </span>
              </div>

              <div className="move-panel">
                <div className="panel-header move-panel-header">
                  <h2>Moves</h2>
                  <span>{selectedWatchReplay.moves_uci.length} total</span>
                </div>
                <MoveList moves={selectedWatchReplay.moves_uci} activePly={selectedPly} latestPly={replayLatestPly} />
              </div>
            </div>
          </div>
        </section>
      ) : selectedLiveMatch?.watch_state === "unavailable" ? (
        <section className="panel watch-panel">
          <EmptyState>
            This match is not live-watchable right now. Its live state is unavailable, and no finished replay was
            found.
          </EmptyState>
        </section>
      ) : !selectedLiveMatch && !rawLiveGame ? (
        <section className="panel watch-panel">
          <EmptyState>
            Live state for this match is not available right now. The match may have already finished or the feed has
            not started yet.
          </EmptyState>
        </section>
      ) : rawLiveGame ? (
        <section className="panel watch-panel">
          <div className="watch-stage">
            <div className="watch-board-column">
              <div className="watch-meta-bar">
                <div>
                  <strong>
                    {participantName(liveWhiteParticipant, "White")} vs {participantName(liveBlackParticipant, "Black")}
                  </strong>
                  <p>
                    {poolNameById[selectedLiveMatch?.pool_id ?? rawLiveGame?.pool_id ?? ""] ?? "Unknown format"} •{" "}
                    {selectedLiveMatch?.interactive
                      ? "Human game"
                      : selectedLiveMatch
                        ? roundLabel(selectedLiveTournament?.kind ?? "round_robin", selectedLiveMatch.round_index)
                        : "Live match"}
                  </p>
                </div>
                <StatusBadge tone={statusTone(visibleLiveStatus)}>{formatLabel(visibleLiveStatus || "running")}</StatusBadge>
              </div>

              {terminalVisibleLive ? (
                <section
                  className={`watch-outcome-hero watch-outcome-${visibleLiveResult ?? "none"} ${resultReveal ? "watch-outcome-reveal" : ""}`}
                >
                  <div>
                    <p className="eyebrow">Match Finished</p>
                    <h2>{outcomeHeadline(visibleLiveResult)}</h2>
                    <p className="watch-outcome-copy">{outcomeSubtitle(visibleLiveResult, visibleLiveTermination)}</p>
                    <p className="watch-outcome-footnote">Replay details are loading while the final position stays on screen.</p>
                  </div>
                  <div className="watch-outcome-actions">
                    <button
                      type="button"
                      className="button-ghost"
                      disabled={!reviewReplayHref}
                      onClick={() => (reviewReplayHref ? navigate(reviewReplayHref) : undefined)}
                    >
                      {reviewReplayHref ? "Review replay" : "Replay loading"}
                    </button>
                    <button
                      type="button"
                      className="button-ghost"
                      onClick={() => navigate(selectedLiveMatch?.interactive ? "/play-engine" : "/live-duel")}
                    >
                      {selectedLiveMatch?.interactive ? "Play again" : "Back to arena"}
                    </button>
                  </div>
                </section>
              ) : null}

              {liveBoardSquares.length > 0 ? (
                <div
                  className={`watch-board-wrap ${boardEventFlash ? "watch-board-wrap-flash" : ""} ${
                    visibleWinnerSide
                      ? `watch-board-wrap-winner-${visibleWinnerSide}`
                      : visibleLiveResult === "draw"
                        ? "watch-board-wrap-draw"
                        : ""
                  }`}
                >
                  <BoardView
                    squares={orientedLiveBoardSquares}
                    selectedSquare={selectedBoardSquare}
                    legalMoveMarkers={selectedSquareMarkers}
                    invalidSquare={invalidBoardSquare}
                    interactive={interactiveLive && rawLiveGame.human_turn && !isSubmittingHumanMove}
                    hoverableSquares={selectableSquares}
                    onSquareClick={handleBoardSquareClick}
                    orientation={liveBoardOrientation}
                  />
                </div>
              ) : (
                <EmptyState>Live board display is unavailable for this game.</EmptyState>
              )}

              <div className="watch-controls">
                <div className="scrubber-row">
                  <span>
                    Ply {visibleLivePly} / {revealedLiveFrames.at(-1)?.ply ?? 0}
                  </span>
                  <input
                    type="range"
                    min={0}
                    max={maxDisplayedLiveFrameIndex}
                    value={Math.min(visibleLiveFrameIndex, maxDisplayedLiveFrameIndex)}
                    onChange={(event) => livePlayback.setSelectedLivePly(Number(event.target.value))}
                  />
                </div>
                <div className="watch-live-controls">
                  <StatusBadge tone={terminalVisibleLive ? "quiet" : liveSideToMove === "white" ? "quiet" : "warning"}>
                    {terminalVisibleLive
                      ? outcomeHeadline(visibleLiveResult)
                      : liveSideToMove === "white"
                        ? whiteUrgency === "critical"
                          ? "White under time pressure"
                          : "White to move"
                        : blackUrgency === "critical"
                          ? "Black under time pressure"
                          : "Black to move"}
                  </StatusBadge>
                  <span className="subtle">{liveStatusMessage()}</span>
                  {!isLiveFollowing ? (
                    <button type="button" className="button-ghost" onClick={livePlayback.returnToLive}>
                      Return to live
                    </button>
                  ) : null}
                </div>
              </div>
            </div>

            <div className="watch-info-column">
              <div className="watch-side-grid">
                <EngineSideCard
                  side="white"
                  title={liveWhiteParticipant?.kind === "human_player" ? "You" : "White engine"}
                  name={participantName(liveWhiteParticipant, "White")}
                  clock={formatClock(displayedWhiteClockMs)}
                  active={liveSideToMove === "white"}
                  urgency={whiteUrgency}
                  winner={visibleWinnerSide === "white"}
                />
                <EngineSideCard
                  side="black"
                  title={liveBlackParticipant?.kind === "human_player" ? "You" : "Black engine"}
                  name={participantName(liveBlackParticipant, "Black")}
                  clock={formatClock(displayedBlackClockMs)}
                  active={liveSideToMove === "black"}
                  urgency={blackUrgency}
                  winner={visibleWinnerSide === "black"}
                />
              </div>

              <div className="watch-stats-grid">
                <StatCard label="Visible plies" value={String(displayedLiveMoves.length)} />
                <StatCard label="Updated" value={visibleLiveFrame ? formatRelativeTime(visibleLiveUpdatedAtMs) : "--"} />
                <StatCard label="Result" value={matchResultText(visibleLiveResult)} />
              </div>

              <div className="result-strip">
                <strong>Live summary</strong>
                <span>
                  {winnerText(visibleLiveResult)}
                  {visibleLiveTermination ? ` via ${formatLabel(visibleLiveTermination)}` : ""}
                </span>
              </div>

              <div className="move-panel">
                <div className="panel-header move-panel-header">
                  <h2>Moves</h2>
                  <span>{displayedLiveMoves.length} revealed</span>
                </div>
                <MoveList
                  moves={displayedLiveMoves}
                  activePly={visibleLivePly}
                  latestPly={visibleLatestPly}
                  animateLatest={latestMoveFlash}
                />
              </div>
            </div>
          </div>
        </section>
      ) : (
        <section className="panel watch-panel">
          <EmptyState>Loading match viewer.</EmptyState>
        </section>
      )}
    </div>
  );
}

function urgencyForClock(ms: number, active: boolean) {
  if (!active) {
    return "normal" as const;
  }
  if (ms <= 5000) {
    return "critical" as const;
  }
  if (ms <= 15000) {
    return "warning" as const;
  }
  return "normal" as const;
}

function selectableHumanPieceSquares(squares: string[], sideToMove: string) {
  if (sideToMove !== "white" && sideToMove !== "black") {
    return new Set<string>();
  }
  const wantsWhite = sideToMove === "white";
  return new Set(
    squares.flatMap((piece, index) => {
      if (!piece) {
        return [];
      }
      const isWhitePiece = piece === piece.toUpperCase();
      return isWhitePiece === wantsWhite ? [squareName(index)] : [];
    })
  );
}
