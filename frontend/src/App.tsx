import { FormEvent, useEffect, useState } from "react";

import { apiUrl, fetchJson } from "./app/api";
import {
  BoardView,
  EmptyState,
  EngineDocumentation,
  EngineSideCard,
  Field,
  MetricCard,
  MoveList,
  StatCard,
  StatusBadge,
  TournamentMapCard
} from "./app/components";
import type {
  Agent,
  AgentVersion,
  BenchmarkPool,
  BoardMoveMarker,
  EventPreset,
  GameRecord,
  HumanPlayerProfile,
  LeaderboardEntry,
  LiveGameState,
  MatchSeries,
  ReplayPayload,
  Variant,
  WorkspaceView,
  Tournament
} from "./app/types";
import {
  buildReplayFrames,
  boardIndexToSquare,
  fenToBoard,
  formatClock,
  formatLabel,
  formatRelativeTime,
  formatTimeControl,
  formatTournamentKind,
  formatVariant,
  isTerminalLiveStatus,
  lastWatchedKey,
  legalMovesByOrigin,
  liveRevealDelayMs,
  loadErrorMessage,
  maybePromotion,
  engineHash,
  navigateToHash,
  orientSquares,
  parseRoute,
  participantName,
  roundLabel,
  squareName,
  statusTone,
  viewHash,
  watchHash,
  workspaceViews,
  matchResultText,
  winnerText
} from "./app/utils";

export default function App() {
  const [agents, setAgents] = useState<Agent[]>([]);
  const [versions, setVersions] = useState<AgentVersion[]>([]);
  const [pools, setPools] = useState<BenchmarkPool[]>([]);
  const [eventPresets, setEventPresets] = useState<EventPreset[]>([]);
  const [tournaments, setTournaments] = useState<Tournament[]>([]);
  const [matches, setMatches] = useState<MatchSeries[]>([]);
  const [leaderboard, setLeaderboard] = useState<LeaderboardEntry[]>([]);
  const [games, setGames] = useState<GameRecord[]>([]);
  const [selectedPoolId, setSelectedPoolId] = useState("");
  const [selectedGameId, setSelectedGameId] = useState("");
  const [selectedPly, setSelectedPly] = useState(0);
  const [replay, setReplay] = useState<ReplayPayload | null>(null);
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [duelName, setDuelName] = useState("");
  const [duelPoolId, setDuelPoolId] = useState("");
  const [duelWhiteId, setDuelWhiteId] = useState("");
  const [duelBlackId, setDuelBlackId] = useState("");
  const [humanGameName, setHumanGameName] = useState("");
  const [humanPoolId, setHumanPoolId] = useState("");
  const [humanEngineId, setHumanEngineId] = useState("");
  const [humanSide, setHumanSide] = useState<"white" | "black" | "random">("random");
  const [humanProfile, setHumanProfile] = useState<HumanPlayerProfile | null>(null);
  const [selectedBoardSquare, setSelectedBoardSquare] = useState("");
  const [invalidBoardSquare, setInvalidBoardSquare] = useState("");
  const [isSubmittingHumanMove, setIsSubmittingHumanMove] = useState(false);
  const [selectedLiveMatchId, setSelectedLiveMatchId] = useState("");
  const [rawLiveGame, setRawLiveGame] = useState<LiveGameState | null>(null);
  const [liveNowMs, setLiveNowMs] = useState(() => Date.now());
  const [displayedLiveFrameCount, setDisplayedLiveFrameCount] = useState(0);
  const [selectedLivePly, setSelectedLivePly] = useState(0);
  const [isLiveFollowing, setIsLiveFollowing] = useState(true);
  const [locationHash, setLocationHash] = useState(() => window.location.hash);
  const [lastWatchedMatchId, setLastWatchedMatchId] = useState(() => {
    try {
      return window.localStorage.getItem(lastWatchedKey) ?? "";
    } catch {
      return "";
    }
  });

  const route = parseRoute(locationHash);
  const activeView = route.page === "app" ? route.view : route.page === "engine" ? "setup" : "live_duel";
  const agentNameById = Object.fromEntries(agents.map((agent) => [agent.id, agent.name]));
  const poolNameById = Object.fromEntries(pools.map((pool) => [pool.id, pool.name]));
  const versionNameById = Object.fromEntries(
    versions.map((version) => [
      version.id,
      `${version.declared_name ?? agentNameById[version.agent_id] ?? "Engine"} ${version.version}`
    ])
  );
  const standardPools = pools.filter((pool) => pool.variant === "standard");
  const tournamentById = Object.fromEntries(tournaments.map((tournament) => [tournament.id, tournament]));
  const gameByMatchId = Object.fromEntries(games.map((game) => [game.match_id, game]));
  const documentedVersions = versions.filter((version) => Boolean(version.documentation?.trim()));
  const recentGames = games.slice(0, 6);
  const runningTournaments = tournaments.filter((tournament) => tournament.status === "running");
  const runningMatches = matches.filter((match) => match.status === "running");
  const completedTournaments = tournaments.filter((tournament) => tournament.status === "completed");
  const activePool = pools.find((pool) => pool.id === selectedPoolId) ?? pools[0];
  const replayFrames = buildReplayFrames(replay);
  const currentFen = replayFrames[Math.min(selectedPly, Math.max(replayFrames.length - 1, 0))];
  const boardSquares = currentFen ? fenToBoard(currentFen) : [];

  const allLiveFrames = rawLiveGame?.live_frames ?? [];
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
  const liveDelayActive = rawLiveGame !== null && displayedLiveFrameCount < allLiveFrames.length;
  const visibleLiveUpdatedAtMs = visibleLiveFrame ? new Date(visibleLiveFrame.updated_at).getTime() : 0;
  const runningClockElapsedMs =
    visibleLiveFrame && visibleLiveFrame.status === "running"
      ? Math.max(0, liveNowMs - visibleLiveUpdatedAtMs)
      : 0;
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
  const liveSideToMove = visibleLiveFrame?.side_to_move ?? "white";
  const selectedLiveMatch = matches.find((match) => match.id === selectedLiveMatchId) ?? null;
  const selectedEngineVersion =
    route.page === "engine" ? versions.find((version) => version.id === route.engineId) ?? null : null;
  const liveWhiteParticipant = rawLiveGame?.white_participant ?? selectedLiveMatch?.white_participant ?? null;
  const liveBlackParticipant = rawLiveGame?.black_participant ?? selectedLiveMatch?.black_participant ?? null;
  const interactiveLive = rawLiveGame?.interactive ?? selectedLiveMatch?.interactive ?? false;
  const liveBoardOrientation =
    interactiveLive && liveBlackParticipant?.kind === "human_player" ? "black" : "white";
  const orientedLiveBoardSquares = orientSquares(liveBoardSquares, liveBoardOrientation);
  const legalMovesForCurrentPosition = rawLiveGame ? legalMovesByOrigin(rawLiveGame.current_fen) : new Map<string, BoardMoveMarker[]>();
  const selectedSquareMarkers = selectedBoardSquare ? legalMovesForCurrentPosition.get(selectedBoardSquare) ?? [] : [];
  const selectableSquares =
    interactiveLive && rawLiveGame?.human_turn
      ? new Set(legalMovesForCurrentPosition.keys())
      : new Set<string>();
  const selectedLiveTournament = selectedLiveMatch
    ? tournamentById[selectedLiveMatch.tournament_id]
    : rawLiveGame
      ? tournamentById[rawLiveGame.tournament_id]
      : undefined;
  const pendingLiveMatch =
    route.page === "watch" &&
    ((selectedLiveMatch?.status === "running" && !rawLiveGame) ||
      (!selectedLiveMatch && !rawLiveGame && !!route.matchId));
  const resumableMatch = runningMatches.find((match) => match.id === lastWatchedMatchId) ?? null;
  const sortedTournaments = [...tournaments].sort((left, right) => {
    const leftRunning = left.status === "running" ? 1 : 0;
    const rightRunning = right.status === "running" ? 1 : 0;
    if (leftRunning !== rightRunning) {
      return rightRunning - leftRunning;
    }
    return left.name.localeCompare(right.name);
  });

  function navigateToView(view: WorkspaceView) {
    navigateToHash(viewHash(view));
  }

  function navigateToWatch(matchId: string) {
    setSelectedLiveMatchId(matchId);
    navigateToHash(watchHash(matchId));
  }

  function navigateToEngine(engineId: string) {
    navigateToHash(engineHash(engineId));
  }

  async function refreshArena(options: { silent?: boolean } = {}) {
    if (!options.silent) {
      setIsLoading(true);
    }

    try {
      const [
        agentsResponse,
        poolsResponse,
        eventPresetsResponse,
        tournamentsResponse,
        matchesResponse,
        gamesResponse,
        leaderboardResponse,
        humanProfileResponse
      ] = await Promise.all([
        fetchJson<Agent[]>("/agents"),
        fetchJson<BenchmarkPool[]>("/pools"),
        fetchJson<EventPreset[]>("/event-presets"),
        fetchJson<Tournament[]>("/tournaments"),
        fetchJson<MatchSeries[]>("/matches"),
        fetchJson<GameRecord[]>("/games"),
        fetchJson<LeaderboardEntry[]>(
          selectedPoolId ? `/leaderboards?pool_id=${selectedPoolId}` : "/leaderboards"
        ),
        fetchJson<HumanPlayerProfile>("/human-player")
      ]);

      const versionResponses = await Promise.all(
        agentsResponse.map((agent) => fetchJson<AgentVersion[]>(`/agents/${agent.id}/versions`))
      );

      setAgents(agentsResponse);
      setVersions(versionResponses.flat());
      setPools(poolsResponse);
      setEventPresets(eventPresetsResponse);
      setTournaments(tournamentsResponse);
      setMatches(matchesResponse);
      setGames(gamesResponse);
      setLeaderboard(leaderboardResponse);
      setHumanProfile(humanProfileResponse);
    } finally {
      if (!options.silent) {
        setIsLoading(false);
      }
    }
  }

  useEffect(() => {
    const handleHashChange = () => {
      setLocationHash(window.location.hash);
    };

    window.addEventListener("hashchange", handleHashChange);
    return () => window.removeEventListener("hashchange", handleHashChange);
  }, []);

  useEffect(() => {
    let cancelled = false;

    const loadArena = async (silent = false) => {
      try {
        await refreshArena({ silent });
      } catch (loadError) {
        if (!cancelled) {
          setError(loadErrorMessage(loadError));
        }
      }
    };

    void loadArena();
    const timer = window.setInterval(() => {
      void loadArena(true);
    }, 3000);

    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [selectedPoolId]);

  useEffect(() => {
    if (!duelPoolId && pools[0]) {
      setDuelPoolId(pools[0].id);
    }
  }, [duelPoolId, pools]);

  useEffect(() => {
    if (!humanPoolId && standardPools[0]) {
      setHumanPoolId(standardPools[0].id);
    }
  }, [humanPoolId, standardPools]);

  useEffect(() => {
    if (!duelWhiteId && versions[0]) {
      setDuelWhiteId(versions[0].id);
    }

    if (!duelBlackId && versions[1]) {
      setDuelBlackId(versions[1].id);
    } else if (!duelBlackId && versions[0]) {
      setDuelBlackId(versions[0].id);
    }
  }, [duelBlackId, duelWhiteId, versions]);

  useEffect(() => {
    if (!humanEngineId && versions[0]) {
      setHumanEngineId(versions[0].id);
    }
  }, [humanEngineId, versions]);

  useEffect(() => {
    if (route.page === "watch" && route.matchId && route.matchId !== selectedLiveMatchId) {
      setSelectedLiveMatchId(route.matchId);
    }
  }, [route, selectedLiveMatchId]);

  useEffect(() => {
    if (route.page === "watch") {
      return;
    }

    if (runningMatches.length === 0) {
      setSelectedLiveMatchId("");
      setRawLiveGame(null);
      setDisplayedLiveFrameCount(0);
      setSelectedLivePly(0);
      setIsLiveFollowing(true);
      return;
    }

    if (!selectedLiveMatchId || !runningMatches.some((match) => match.id === selectedLiveMatchId)) {
      setSelectedLiveMatchId(runningMatches[0].id);
      setSelectedLivePly(0);
      setIsLiveFollowing(true);
    }
  }, [route.page, runningMatches, selectedLiveMatchId]);

  useEffect(() => {
    if (!selectedLiveMatchId) {
      return;
    }

    try {
      window.localStorage.setItem(lastWatchedKey, selectedLiveMatchId);
      setLastWatchedMatchId(selectedLiveMatchId);
    } catch {
      // Ignore storage failures in sandboxed or private contexts.
    }
  }, [selectedLiveMatchId]);

  useEffect(() => {
    if (!selectedLiveMatchId) {
      setRawLiveGame(null);
      return;
    }

    let cancelled = false;
    let reconnectTimer: number | null = null;
    let eventSource: EventSource | null = null;
    let terminalStateSeen = false;
    let pollTimer: number | null = null;

    const closeStream = () => {
      if (eventSource) {
        eventSource.close();
        eventSource = null;
      }
    };

    const loadLiveSnapshot = async () => {
      try {
        const state = await fetchJson<LiveGameState>(`/matches/${selectedLiveMatchId}/live`);
        if (!cancelled) {
          setRawLiveGame(state);
          if (isTerminalLiveStatus(state.status)) {
            terminalStateSeen = true;
          }
        }
      } catch {
        if (!cancelled && !terminalStateSeen) {
          setRawLiveGame(null);
        }
      }
    };

    const schedulePoll = () => {
      if (cancelled || terminalStateSeen || pollTimer !== null) {
        return;
      }

      pollTimer = window.setTimeout(() => {
        pollTimer = null;
        void loadLiveSnapshot().finally(() => {
          schedulePoll();
        });
      }, 2000);
    };

    const scheduleReconnect = () => {
      if (cancelled || terminalStateSeen || reconnectTimer !== null) {
        return;
      }

      reconnectTimer = window.setTimeout(() => {
        reconnectTimer = null;
        connectStream();
      }, 1000);
    };

    const connectStream = () => {
      if (cancelled || terminalStateSeen) {
        return;
      }

      closeStream();
      const source = new EventSource(apiUrl(`/matches/${selectedLiveMatchId}/live/stream`));
      eventSource = source;

      const handleLiveGameEvent = (event: MessageEvent<string>) => {
        try {
          const state = JSON.parse(event.data) as LiveGameState;
          if (cancelled) {
            return;
          }

          setRawLiveGame(state);
          if (isTerminalLiveStatus(state.status)) {
            terminalStateSeen = true;
            closeStream();
          }
        } catch {
          // Ignore malformed events and wait for the next update or fallback snapshot.
        }
      };

      source.addEventListener("live_game", handleLiveGameEvent);
      source.onerror = () => {
        closeStream();
        void loadLiveSnapshot();
        scheduleReconnect();
      };
    };

    void loadLiveSnapshot();
    connectStream();
    schedulePoll();

    return () => {
      cancelled = true;
      closeStream();
      if (reconnectTimer !== null) {
        window.clearTimeout(reconnectTimer);
      }
      if (pollTimer !== null) {
        window.clearTimeout(pollTimer);
      }
    };
  }, [selectedLiveMatchId]);

  useEffect(() => {
    if (route.page !== "watch" || !route.matchId || rawLiveGame || selectedLiveMatch?.status !== "running") {
      return;
    }

    let cancelled = false;

    const pollUntilLive = async () => {
      while (!cancelled) {
        try {
          const state = await fetchJson<LiveGameState>(`/matches/${route.matchId}/live`);
          if (!cancelled) {
            setRawLiveGame(state);
          }
          return;
        } catch {
          await new Promise<void>((resolve) => {
            window.setTimeout(resolve, 300);
          });
        }
      }
    };

    void pollUntilLive();

    return () => {
      cancelled = true;
    };
  }, [route, rawLiveGame, selectedLiveMatch?.status]);

  useEffect(() => {
    if (!rawLiveGame || rawLiveGame.status !== "running") {
      return;
    }

    const timer = window.setInterval(() => {
      setLiveNowMs(Date.now());
    }, 250);

    return () => window.clearInterval(timer);
  }, [visibleLiveFrame?.ply, visibleLiveFrame?.status, visibleLiveFrame?.updated_at]);

  useEffect(() => {
    if (!rawLiveGame) {
      setDisplayedLiveFrameCount(0);
      setSelectedLivePly(0);
      setIsLiveFollowing(true);
      setSelectedBoardSquare("");
      setInvalidBoardSquare("");
      return;
    }

    const initialVisibleFrames = rawLiveGame.interactive ? rawLiveGame.live_frames.length : Math.min(rawLiveGame.live_frames.length, 1);
    setDisplayedLiveFrameCount(initialVisibleFrames);
    setSelectedLivePly(Math.max(initialVisibleFrames - 1, 0));
    setIsLiveFollowing(true);
    setSelectedBoardSquare("");
    setInvalidBoardSquare("");
  }, [rawLiveGame?.match_id]);

  useEffect(() => {
    if (rawLiveGame?.interactive) {
      setDisplayedLiveFrameCount(rawLiveGame.live_frames.length);
      if (isLiveFollowing) {
        setSelectedLivePly(Math.max(rawLiveGame.live_frames.length - 1, 0));
      }
    }
  }, [rawLiveGame?.interactive, rawLiveGame?.live_frames.length, isLiveFollowing]);

  useEffect(() => {
    if (!invalidBoardSquare) {
      return;
    }

    const timer = window.setTimeout(() => {
      setInvalidBoardSquare("");
    }, 280);

    return () => window.clearTimeout(timer);
  }, [invalidBoardSquare]);

  useEffect(() => {
    if (!rawLiveGame && displayedLiveFrameCount !== 0) {
      setDisplayedLiveFrameCount(0);
      return;
    }

    if (rawLiveGame && displayedLiveFrameCount > rawLiveGame.live_frames.length) {
      setDisplayedLiveFrameCount(rawLiveGame.live_frames.length);
    }
  }, [rawLiveGame, displayedLiveFrameCount]);

  useEffect(() => {
    if (!rawLiveGame) {
      return;
    }

    if (rawLiveGame.interactive) {
      return;
    }

    if (displayedLiveFrameCount >= rawLiveGame.live_frames.length) {
      return;
    }

    const timer = window.setTimeout(() => {
      setDisplayedLiveFrameCount((current) => Math.min(current + 1, rawLiveGame.live_frames.length));
    }, liveRevealDelayMs);

    return () => window.clearTimeout(timer);
  }, [rawLiveGame, displayedLiveFrameCount]);

  useEffect(() => {
    if (isLiveFollowing) {
      setSelectedLivePly(maxDisplayedLiveFrameIndex);
    }
  }, [isLiveFollowing, maxDisplayedLiveFrameIndex]);

  useEffect(() => {
    if (!isLiveFollowing && selectedLivePly > maxDisplayedLiveFrameIndex) {
      setSelectedLivePly(maxDisplayedLiveFrameIndex);
    }
  }, [isLiveFollowing, selectedLivePly, maxDisplayedLiveFrameIndex]);

  async function withRefresh(action: () => Promise<void>, success: string) {
    try {
      setError("");
      await action();
      setNotice(success);
      await refreshArena();
    } catch (actionError) {
      setNotice("");
      setError(loadErrorMessage(actionError));
    }
  }

  async function startEventPreset(id: string) {
    await withRefresh(async () => {
      await fetchJson(`/event-presets/${id}/start`, { method: "POST" });
      navigateToView("events");
    }, "Event started. Live results refresh automatically.");
  }

  async function loadReplay(id: string) {
    navigateToView("replay");
    setSelectedGameId(id);
    setSelectedPly(0);
    setReplay(null);
    setError("");
    try {
      const replayResponse = await fetchJson<ReplayPayload>(`/games/${id}/replay`);
      setReplay(replayResponse);
    } catch (loadError) {
      setError(loadErrorMessage(loadError));
    }
  }

  async function waitForTournamentMatch(tournamentId: string) {
    const deadline = Date.now() + 8000;

    while (Date.now() < deadline) {
      const tournamentMatches = await fetchJson<MatchSeries[]>(
        `/matches?tournament_id=${encodeURIComponent(tournamentId)}`
      );
      const liveMatch = tournamentMatches.find((match) => match.status === "running") ?? tournamentMatches[0];
      if (liveMatch) {
        return liveMatch.id;
      }

      await new Promise<void>((resolve) => {
        window.setTimeout(resolve, 250);
      });
    }

    return "";
  }

  async function submitLiveDuel(event: FormEvent) {
    event.preventDefault();

    if (!duelWhiteId || !duelBlackId) {
      setNotice("");
      setError("Pick two engines for the live duel.");
      return;
    }

    if (duelWhiteId === duelBlackId) {
      setNotice("");
      setError("Pick two different engines for the live duel.");
      return;
    }

    const whiteName = versionNameById[duelWhiteId] ?? "White";
    const blackName = versionNameById[duelBlackId] ?? "Black";
    const name = duelName.trim() || `${whiteName} vs ${blackName}`;

    await withRefresh(async () => {
      const response = await fetchJson<{ tournament_id: string }>("/duels", {
        method: "POST",
        body: JSON.stringify({
          name,
          pool_id: duelPoolId,
          white_version_id: duelWhiteId,
          black_version_id: duelBlackId
        })
      });
      setDuelName("");
      const matchId = await waitForTournamentMatch(response.tournament_id);
      if (matchId) {
        navigateToWatch(matchId);
      } else {
        navigateToView("live_duel");
      }
    }, "Live duel started. Elo will update when the game finishes.");
  }

  async function submitHumanGame(event: FormEvent) {
    event.preventDefault();

    if (!humanPoolId || !humanEngineId) {
      setNotice("");
      setError("Pick a standard format and an engine first.");
      return;
    }

    const engineName = versionNameById[humanEngineId] ?? "Engine";
    const chosenName = humanGameName.trim() || `You vs ${engineName}`;

    await withRefresh(async () => {
      const response = await fetchJson<{ match_id: string }>("/human-games", {
        method: "POST",
        body: JSON.stringify({
          name: chosenName,
          pool_id: humanPoolId,
          engine_version_id: humanEngineId,
          human_side: humanSide
        })
      });
      setHumanGameName("");
      setSelectedBoardSquare("");
      navigateToWatch(response.match_id);
    }, "Human game started. Your Elo will update when the game finishes.");
  }

  async function submitHumanMove(uci: string) {
    if (!rawLiveGame) {
      return;
    }

    setIsSubmittingHumanMove(true);
    setError("");
    try {
      await fetchJson(`/human-games/${rawLiveGame.match_id}/move`, {
        method: "POST",
        body: JSON.stringify({ uci })
      });
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
    if (!legalDestination) {
      setInvalidBoardSquare(square);
      return;
    }

    const fromIndex = liveBoardSquares.findIndex((_, boardIndex) => squareName(boardIndex) === selectedBoardSquare);
    const fromPiece = fromIndex >= 0 ? liveBoardSquares[fromIndex] : "";
    void submitHumanMove(`${selectedBoardSquare}${square}${maybePromotion(selectedBoardSquare, square, fromPiece)}`);
  }

  if (route.page === "watch") {
    return (
      <div className="watch-shell">
        <header className="watch-header">
          <div className="watch-header-copy">
            <p className="eyebrow">{interactiveLive ? "Play vs Engine" : "Live Watch"}</p>
            <h1>{interactiveLive ? "Fullscreen human match" : "Fullscreen engine viewer"}</h1>
            <p className="lede">
              {interactiveLive
                ? "Play directly on the board, watch the engine answer, and keep the clocks and move list in view."
                : "Follow one match at a readable pace with clear White and Black panels and a slightly delayed move feed."}
            </p>
          </div>
          <div className="watch-header-actions">
            <button
              type="button"
              className="button-ghost"
              onClick={() => navigateToView(interactiveLive ? "play_engine" : "live_duel")}
            >
              Back to arena
            </button>
            {selectedLiveMatch ? (
              <StatusBadge tone={statusTone(visibleLiveStatus)}>
                {selectedLiveMatch.interactive
                  ? "Human game"
                  : roundLabel(selectedLiveTournament?.kind ?? "round_robin", selectedLiveMatch.round_index)}
              </StatusBadge>
            ) : null}
          </div>
        </header>

        {error && <section className="banner banner-error">{error}</section>}

        {pendingLiveMatch ? (
          <section className="panel watch-panel">
            <EmptyState>
              Preparing the live board. The match exists, and the viewer is waiting for the first live state to arrive.
            </EmptyState>
          </section>
        ) : !selectedLiveMatch && !rawLiveGame ? (
          <section className="panel watch-panel">
            <EmptyState>
              Live state for this match is not available right now. The match may have already finished or the feed
              has not started yet.
            </EmptyState>
          </section>
        ) : (
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
                  <StatusBadge tone={statusTone(visibleLiveStatus)}>
                    {formatLabel(visibleLiveStatus || "running")}
                  </StatusBadge>
                </div>

                {rawLiveGame?.variant === "standard" && liveBoardSquares.length > 0 ? (
                  <div className="watch-board-wrap">
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
                  <EmptyState>
                    Live board display is available for standard games. Chess960 still updates the move list and
                    clocks.
                  </EmptyState>
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
                      onChange={(event) => {
                        const next = Number(event.target.value);
                        setSelectedLivePly(next);
                        setIsLiveFollowing(next >= maxDisplayedLiveFrameIndex);
                      }}
                    />
                  </div>
                  <div className="watch-live-controls">
                    <StatusBadge tone={liveSideToMove === "white" ? "quiet" : "warning"}>
                      {liveSideToMove === "white" ? "White to move" : "Black to move"}
                    </StatusBadge>
                    <span className="subtle">
                      {interactiveLive
                        ? rawLiveGame?.human_turn
                          ? isSubmittingHumanMove
                            ? "Submitting move..."
                            : "Your turn: click a piece, then click its destination."
                          : "Engine is thinking."
                        : "Viewer is delayed slightly for readability."}
                    </span>
                    {!isLiveFollowing ? (
                      <button type="button" className="button-ghost" onClick={() => setIsLiveFollowing(true)}>
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
                    clock={rawLiveGame ? formatClock(displayedWhiteClockMs) : "--:--"}
                    active={liveSideToMove === "white"}
                  />
                  <EngineSideCard
                    side="black"
                    title={liveBlackParticipant?.kind === "human_player" ? "You" : "Black engine"}
                    name={participantName(liveBlackParticipant, "Black")}
                    clock={rawLiveGame ? formatClock(displayedBlackClockMs) : "--:--"}
                    active={liveSideToMove === "black"}
                  />
                </div>

                <div className="watch-stats-grid">
                  <StatCard label="Visible plies" value={String(displayedLiveMoves.length)} />
                  <StatCard
                    label="Updated"
                    value={visibleLiveFrame ? formatRelativeTime(visibleLiveUpdatedAtMs) : "--"}
                  />
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
                  <MoveList moves={displayedLiveMoves} activePly={visibleLivePly} />
                </div>
              </div>
            </div>
          </section>
        )}
      </div>
    );
  }

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
          <MetricCard label="Families" value={String(agents.length)} />
          <MetricCard label="Engines" value={String(versions.length)} />
          <MetricCard label="Finished Games" value={String(games.length)} />
        </div>
      </header>

      <nav className="view-switcher" aria-label="Arena sections">
        {workspaceViews.map((view) => (
          <button
            key={view.id}
            type="button"
            className={`view-button ${activeView === view.id ? "view-button-active" : ""}`}
            onClick={() => navigateToView(view.id)}
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
        {activeView === "overview" && (
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
                finished games.
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
                  <strong>{eventPresets.length}</strong>
                  <p>
                    {completedTournaments.length} completed runs, {runningTournaments.length} running
                  </p>
                </div>
              </div>
              <div className="quick-actions">
                <button type="button" className="button-ghost" onClick={() => navigateToView("setup")}>
                  Browse engines
                </button>
                <button type="button" className="button-ghost" onClick={() => navigateToView("live_duel")}>
                  Start live duel
                </button>
                <button type="button" className="button-ghost" onClick={() => navigateToView("play_engine")}>
                  Play vs engine
                </button>
                <button type="button" className="button-ghost" onClick={() => navigateToView("events")}>
                  Start event
                </button>
                <button type="button" className="button-ghost" onClick={() => navigateToView("tournament")}>
                  Open tournament map
                </button>
                <button type="button" className="button-ghost" onClick={() => navigateToView("replay")}>
                  Watch replay
                </button>
              </div>
            </section>

            <section className="panel">
              <div className="panel-header">
                <h2>Recent Games</h2>
                <span>{recentGames.length} latest</span>
              </div>
              {recentGames.length === 0 ? (
                <EmptyState>No finished games yet. Start an event and activity will show up here.</EmptyState>
              ) : (
                <div className="table">
                  {recentGames.map((game) => (
                    <button
                      type="button"
                      className={`table-row replay-row ${selectedGameId === game.id ? "table-row-active" : ""}`}
                      key={game.id}
                      onClick={() => loadReplay(game.id)}
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
          </>
        )}

        {route.page === "app" && activeView === "setup" && (
          <>
            <section className="panel">
              <div className="panel-header">
                <h2>Available Engines</h2>
                <span>{versions.length} ready to play</span>
              </div>
              <p className="panel-copy">Browse the engines currently available for duels and preset events.</p>

              <div className="summary-grid">
                <div className="summary-card">
                  <span>Engine lines</span>
                  <strong>{agents.length}</strong>
                  <p>Distinct engine families</p>
                </div>
                <div className="summary-card">
                  <span>Playable versions</span>
                  <strong>{versions.length}</strong>
                  <p>Ready to enter</p>
                </div>
                <div className="summary-card">
                  <span>Preset events</span>
                  <strong>{eventPresets.length}</strong>
                  <p>Backend-defined only</p>
                </div>
              </div>

              <div className="section-heading">Engines</div>
              {versions.length === 0 ? (
                <EmptyState>No engine versions are available right now.</EmptyState>
              ) : (
                <div className="table engine-directory-list">
                  {versions.map((version) => (
                    <button
                      type="button"
                      className="table-row table-row-stack replay-row"
                      key={version.id}
                      onClick={() => navigateToEngine(version.id)}
                    >
                      <div>
                        <strong>{versionNameById[version.id]}</strong>
                        <p>{agentNameById[version.agent_id] ?? "Unknown agent"}</p>
                        {version.notes ? <p>{version.notes}</p> : null}
                        <p>Open full engine page</p>
                      </div>
                      <div className="chip">{version.tags.join(" • ") || "Engine"}</div>
                    </button>
                  ))}
                </div>
              )}
            </section>

            <section className="panel">
              <div className="panel-header">
                <h2>Formats</h2>
                <select value={selectedPoolId} onChange={(event) => setSelectedPoolId(event.target.value)}>
                  <option value="">All formats</option>
                  {pools.map((pool) => (
                    <option key={pool.id} value={pool.id}>
                      {pool.name}
                    </option>
                  ))}
                </select>
              </div>
              <p className="panel-copy">Browse the formats available for events.</p>

              <div className="section-heading">Available formats</div>
              {pools.length === 0 ? (
                <EmptyState>No formats are available right now.</EmptyState>
              ) : (
                <div className="table">
                  {pools.map((pool) => (
                    <div className="table-row table-row-stack" key={pool.id}>
                      <div>
                        <strong>{pool.name}</strong>
                        <p>
                          {formatVariant(pool.variant)} • {formatTimeControl(pool.time_control)}
                        </p>
                        <p>
                          {pool.fairness.swap_colors
                            ? "Colors swap between paired games."
                            : "Colors stay as scheduled."}
                        </p>
                        {pool.description ? <p>{pool.description}</p> : null}
                      </div>
                      <div className="chip">{selectedPoolId === pool.id ? "Selected" : "Format"}</div>
                    </div>
                  ))}
                </div>
              )}
            </section>
          </>
        )}

        {activeView === "live_duel" && (
          <>
            <section className="panel">
              <div className="panel-header">
                <h2>Live Duel</h2>
                <span>{runningMatches.length > 0 ? "Watching enabled" : "Ready to launch"}</span>
              </div>
              <p className="panel-copy">
                Pick any two engines, start one live game immediately, and then open the fullscreen watch page to
                follow it comfortably.
              </p>

              <form className="stack" onSubmit={submitLiveDuel}>
                <Field label="Duel name" hint="Optional">
                  <input
                    value={duelName}
                    onChange={(event) => setDuelName(event.target.value)}
                    placeholder="Example: MiniMax vs King Safety"
                  />
                </Field>
                <Field label="Format">
                  <select value={duelPoolId} onChange={(event) => setDuelPoolId(event.target.value)} required>
                    <option value="">Select format</option>
                    {pools.map((pool) => (
                      <option key={pool.id} value={pool.id}>
                        {pool.name}
                      </option>
                    ))}
                  </select>
                </Field>
                <div className="two-up">
                  <Field label="White engine">
                    <select value={duelWhiteId} onChange={(event) => setDuelWhiteId(event.target.value)} required>
                      <option value="">Select engine</option>
                      {versions.map((version) => (
                        <option key={version.id} value={version.id}>
                          {versionNameById[version.id]}
                        </option>
                      ))}
                    </select>
                  </Field>
                  <Field label="Black engine">
                    <select value={duelBlackId} onChange={(event) => setDuelBlackId(event.target.value)} required>
                      <option value="">Select engine</option>
                      {versions.map((version) => (
                        <option key={version.id} value={version.id}>
                          {versionNameById[version.id]}
                        </option>
                      ))}
                    </select>
                  </Field>
                </div>

                <div className="duel-preview">
                  <EngineSideCard
                    side="white"
                    title="White side"
                    name={versionNameById[duelWhiteId] ?? "Choose an engine"}
                  />
                  <EngineSideCard
                    side="black"
                    title="Black side"
                    name={versionNameById[duelBlackId] ?? "Choose an engine"}
                  />
                </div>

                <div className="result-strip">
                  <strong>Match setup</strong>
                  <span>Manual live duel still starts exactly 1 game by default.</span>
                </div>

                <button type="submit">Start live duel</button>
              </form>
            </section>

            <section className="panel replay-panel">
              <div className="panel-header">
                <h2>Live Matches</h2>
                <span>{runningMatches.length > 0 ? `${runningMatches.length} running` : "No live match yet"}</span>
              </div>
              <p className="panel-copy">
                Choose a live match and open the dedicated watch page instead of squeezing the board into this control
                screen.
              </p>

              {resumableMatch ? (
                <div className="resume-banner">
                  <div>
                    <strong>Resume last watched match</strong>
                    <p>
                      White: {participantName(resumableMatch.white_participant, "White")} • Black:{" "}
                      {participantName(resumableMatch.black_participant, "Black")}
                    </p>
                  </div>
                  <button type="button" onClick={() => navigateToWatch(resumableMatch.id)}>
                    Resume fullscreen
                  </button>
                </div>
              ) : null}

              {runningMatches.length === 0 ? (
                <EmptyState>Start a live duel or launch a preset event to watch an active board in fullscreen.</EmptyState>
              ) : (
                <div className="table live-directory">
                  {runningMatches.map((match) => (
                    <div className="table-row table-row-stack live-directory-row" key={match.id}>
                      <div className="live-directory-copy">
                        <div className="live-engine-pair">
                          <div className="side-pill side-pill-white">
                            <span>White</span>
                            <strong>{participantName(match.white_participant, "White")}</strong>
                          </div>
                          <div className="side-pill side-pill-black">
                            <span>Black</span>
                            <strong>{participantName(match.black_participant, "Black")}</strong>
                          </div>
                        </div>
                        <p>
                          {poolNameById[match.pool_id] ?? "Unknown format"} •{" "}
                          {match.interactive
                            ? "Human game"
                            : roundLabel(tournamentById[match.tournament_id]?.kind ?? "round_robin", match.round_index)}
                        </p>
                      </div>
                      <div className="live-directory-actions">
                        <div className="chip">{formatLabel(match.status)}</div>
                        <button type="button" onClick={() => navigateToWatch(match.id)}>
                          Watch fullscreen
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </section>
          </>
        )}

        {activeView === "play_engine" && (
          <section className="panel">
            <div className="panel-header">
              <h2>Play vs Engine</h2>
              <span>{humanProfile ? `Your Elo ${humanProfile.rating.toFixed(1)}` : "Ready to play"}</span>
            </div>
            <p className="panel-copy">
              Pick any engine, choose your side, and play a live standard game that updates both your Elo and the
              engine&apos;s Elo.
            </p>

            <form className="stack" onSubmit={submitHumanGame}>
              <Field label="Game name" hint="Optional">
                <input
                  value={humanGameName}
                  onChange={(event) => setHumanGameName(event.target.value)}
                  placeholder="Example: Me vs MiniMax"
                />
              </Field>
              <Field label="Standard format">
                <select value={humanPoolId} onChange={(event) => setHumanPoolId(event.target.value)} required>
                  <option value="">Select standard format</option>
                  {standardPools.map((pool) => (
                    <option key={pool.id} value={pool.id}>
                      {pool.name}
                    </option>
                  ))}
                </select>
              </Field>
              <div className="two-up">
                <Field label="Engine">
                  <select value={humanEngineId} onChange={(event) => setHumanEngineId(event.target.value)} required>
                    <option value="">Select engine</option>
                    {versions.map((version) => (
                      <option key={version.id} value={version.id}>
                        {versionNameById[version.id]}
                      </option>
                    ))}
                  </select>
                </Field>
                <Field label="Your side">
                  <select
                    value={humanSide}
                    onChange={(event) => setHumanSide(event.target.value as "white" | "black" | "random")}
                  >
                    <option value="white">White</option>
                    <option value="black">Black</option>
                    <option value="random">Random</option>
                  </select>
                </Field>
              </div>

              <div className="duel-preview">
                <EngineSideCard
                  side="white"
                  title={humanSide === "black" ? "White side" : "You"}
                  name={
                    humanSide === "black"
                      ? versionNameById[humanEngineId] ?? "Engine"
                      : humanSide === "random"
                        ? "You or engine"
                        : "You"
                  }
                />
                <EngineSideCard
                  side="black"
                  title={humanSide === "black" ? "You" : "Black side"}
                  name={
                    humanSide === "black"
                      ? "You"
                      : humanSide === "random"
                        ? "You or engine"
                        : versionNameById[humanEngineId] ?? "Choose an engine"
                  }
                />
              </div>

              <div className="result-strip">
                <strong>Human profile</strong>
                <span>
                  {humanProfile
                    ? `${humanProfile.games_played} games • ${humanProfile.wins}W ${humanProfile.draws}D ${humanProfile.losses}L`
                    : "Your profile is ready."}
                </span>
              </div>

              <button type="submit">Start human game</button>
            </form>
          </section>
        )}

        {activeView === "events" && (
          <section className="panel">
            <div className="panel-header">
              <h2>Event Presets</h2>
              <span>{isLoading ? "Refreshing..." : `${eventPresets.length} ready`}</span>
            </div>
            <p className="panel-copy">
              These events are defined in backend setup. Starting one launches a fresh run with the current active
              engine lineup.
            </p>

            <div className="section-heading">Available events</div>
            {eventPresets.length === 0 ? (
              <EmptyState>No backend-defined event presets are available right now.</EmptyState>
            ) : (
              <div className="table">
                {eventPresets.map((preset) => (
                  <div className="table-row" key={preset.id}>
                    <div>
                      <strong>{preset.name}</strong>
                      <p>
                        {formatTournamentKind(preset.kind)} • {poolNameById[preset.pool_id] ?? "Unknown format"} •{" "}
                        {preset.games_per_pairing} game{preset.games_per_pairing === 1 ? "" : "s"} per pairing
                      </p>
                      <p>Selection mode: all active engines • Workers: {preset.worker_count}</p>
                    </div>
                    <button type="button" disabled={!preset.active} onClick={() => startEventPreset(preset.id)}>
                      {preset.active ? "Start" : "Inactive"}
                    </button>
                  </div>
                ))}
              </div>
            )}
          </section>
        )}

        {activeView === "overview" && (
          <section className="panel">
            <div className="panel-header">
              <h2>Standings</h2>
              <div className="panel-header-actions">
                <span>{selectedPoolId ? "Format selected" : "All engines"}</span>
                <button type="button" className="button-ghost compact-button" onClick={() => navigateToView("tournament")}>
                  Tournament map
                </button>
              </div>
            </div>
            <p className="panel-copy">Ratings update as finished games roll in.</p>
            {leaderboard.length === 0 ? (
              <EmptyState>No standings yet.</EmptyState>
            ) : (
              <div className="leaderboard">
                {leaderboard.map((entry, index) => (
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
        )}

        {activeView === "tournament" && (
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
                    matches={matches.filter((match) => match.tournament_id === tournament.id)}
                    gameByMatchId={gameByMatchId}
                    poolNameById={poolNameById}
                    onWatch={navigateToWatch}
                  />
                ))}
              </div>
            )}
          </section>
        )}

        {activeView === "replay" && (
          <section className="panel replay-panel">
            <div className="panel-header">
              <h2>Replay</h2>
              <span>{selectedGameId ? "Game selected" : "Pick a game"}</span>
            </div>
            <div className="table games-feed">
              {games.slice(0, 8).map((game) => (
                <button
                  type="button"
                  className={`table-row replay-row ${selectedGameId === game.id ? "table-row-active" : ""}`}
                  key={game.id}
                  onClick={() => loadReplay(game.id)}
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

            {games.length === 0 && (
              <EmptyState>No games have finished yet. Start an event and this area will fill in automatically.</EmptyState>
            )}

            {!replay && games.length > 0 && (
              <EmptyState>Choose a finished game above to load the board, result, and move list.</EmptyState>
            )}

            {replay && (
              <div className="replay-content">
                {replay.variant === "standard" && boardSquares.length > 0 ? (
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
                      {matchResultText(replay.result)} via {formatLabel(replay.termination)}
                    </span>
                  </div>
                  <Field label="Move record">
                    <textarea className="replay-textarea" readOnly rows={8} value={replay.pgn} />
                  </Field>
                </div>
              </div>
            )}
          </section>
        )}

        {route.page === "engine" && (
          <section className="panel engine-page">
            <div className="panel-header">
              <h2>Engine Page</h2>
              <button type="button" className="button-ghost compact-button" onClick={() => navigateToView("setup")}>
                Back to engines
              </button>
            </div>

            {selectedEngineVersion ? (
              <div className="engine-detail">
                <div className="engine-detail-header">
                  <div>
                    <p className="eyebrow">Engine dossier</p>
                    <h3>{versionNameById[selectedEngineVersion.id]}</h3>
                    <p>{agentNameById[selectedEngineVersion.agent_id] ?? "Unknown agent"}</p>
                  </div>
                  <div className="chip">{selectedEngineVersion.tags.join(" • ") || "Engine"}</div>
                </div>

                {selectedEngineVersion.notes ? (
                  <div className="result-strip">
                    <strong>Summary</strong>
                    <span>{selectedEngineVersion.notes}</span>
                  </div>
                ) : null}

                {selectedEngineVersion.documentation ? (
                  <EngineDocumentation text={selectedEngineVersion.documentation} />
                ) : (
                  <EmptyState>No long-form documentation is available for this engine yet.</EmptyState>
                )}
              </div>
            ) : (
              <EmptyState>This engine page could not be loaded.</EmptyState>
            )}
          </section>
        )}
      </main>
    </div>
  );
}
