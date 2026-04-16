import { FormEvent, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";

import { useFlash } from "../../app/providers/FlashProvider";
import type { Variant } from "../../app/types";
import { formatLabel, formatTimeControl, formatVariant, roundLabel } from "../../shared/lib/format";
import { participantName } from "../../shared/lib/participants";
import { findPoolForChoices, timeControlKey, uniquePoolTimeControls, uniquePoolVariants } from "../../shared/lib/pools";
import { supportsVariant } from "../../shared/lib/variants";
import { useAgentVersionsQuery, useMatchesQuery, usePoolsQuery, useTournamentsQuery, useAgentsQuery } from "../../shared/queries/arena";
import { EmptyState, EngineSideCard, Field, RouteErrorState, RouteLoadingState } from "../../shared/ui";
import { lastWatchedKey } from "../watch/model";
import { useCreateLiveDuelMutation } from "./api";

export function LiveDuelPage() {
  const navigate = useNavigate();
  const { showError, showNotice } = useFlash();
  const agents = useAgentsQuery();
  const versions = useAgentVersionsQuery();
  const pools = usePoolsQuery();
  const matches = useMatchesQuery(3000);
  const tournaments = useTournamentsQuery(3000);
  const createDuel = useCreateLiveDuelMutation();
  const [duelName, setDuelName] = useState("");
  const [duelVariant, setDuelVariant] = useState<Variant | "">("");
  const [duelTimeControlKey, setDuelTimeControlKey] = useState("");
  const [duelWhiteId, setDuelWhiteId] = useState("");
  const [duelBlackId, setDuelBlackId] = useState("");
  const [lastWatchedMatchId] = useState(() => {
    try {
      return window.localStorage.getItem(lastWatchedKey) ?? "";
    } catch {
      return "";
    }
  });

  useEffect(() => {
    const variantChoices = uniquePoolVariants(pools.data ?? []);
    if (!duelVariant && variantChoices[0]) {
      setDuelVariant(variantChoices[0]);
    }
  }, [duelVariant, pools.data]);

  useEffect(() => {
    const timeControlChoices = uniquePoolTimeControls(pools.data ?? []);
    if (!duelTimeControlKey && timeControlChoices[0]) {
      setDuelTimeControlKey(timeControlKey(timeControlChoices[0]));
    }
  }, [duelTimeControlKey, pools.data]);

  const agentNameById = Object.fromEntries((agents.data ?? []).map((agent) => [agent.id, agent.name]));
  const versionNameById = Object.fromEntries(
    (versions.data ?? []).map((version) => [
      version.id,
      `${version.declared_name ?? agentNameById[version.agent_id] ?? "Engine"} ${version.version}`
    ])
  );
  const runningMatches = (matches.data ?? []).filter((match) => match.status === "running" && match.watch_state === "live");
  const tournamentById = Object.fromEntries((tournaments.data ?? []).map((tournament) => [tournament.id, tournament]));
  const poolNameById = Object.fromEntries((pools.data ?? []).map((pool) => [pool.id, pool.name]));
  const variantChoices = uniquePoolVariants(pools.data ?? []);
  const timeControlChoices = uniquePoolTimeControls(pools.data ?? []);
  const selectedPool = duelVariant ? findPoolForChoices(pools.data ?? [], duelVariant, duelTimeControlKey) : null;
  const compatibleVersions = (versions.data ?? []).filter((version) =>
    duelVariant ? supportsVariant(version, duelVariant) : true
  );
  const resumableMatch = runningMatches.find((match) => match.id === lastWatchedMatchId) ?? null;

  useEffect(() => {
    if (duelWhiteId && !compatibleVersions.some((version) => version.id === duelWhiteId)) {
      setDuelWhiteId("");
    }
    if (duelBlackId && !compatibleVersions.some((version) => version.id === duelBlackId)) {
      setDuelBlackId("");
    }
    if (!duelWhiteId && compatibleVersions[0]) {
      setDuelWhiteId(compatibleVersions[0].id);
    }
    if (!duelBlackId && compatibleVersions[1]) {
      setDuelBlackId(compatibleVersions[1].id);
    } else if (!duelBlackId && compatibleVersions[0]) {
      setDuelBlackId(compatibleVersions[0].id);
    }
  }, [compatibleVersions, duelBlackId, duelWhiteId]);

  if (agents.isLoading || versions.isLoading || pools.isLoading || matches.isLoading || tournaments.isLoading) {
    return <RouteLoadingState message="Loading duel controls..." />;
  }

  const error = agents.error ?? versions.error ?? pools.error ?? matches.error ?? tournaments.error;
  if (error) {
    return <RouteErrorState message={error.message} />;
  }

  async function submitLiveDuel(event: FormEvent) {
    event.preventDefault();

    if (!duelWhiteId || !duelBlackId || !selectedPool) {
      showError("Pick a chess type, time control, and two engines for the live duel.");
      return;
    }

    if (duelWhiteId === duelBlackId) {
      showError("Pick two different engines for the live duel.");
      return;
    }

    const whiteName = versionNameById[duelWhiteId] ?? "White";
    const blackName = versionNameById[duelBlackId] ?? "Black";
    const name = duelName.trim() || `${whiteName} vs ${blackName}`;

    try {
      const result = await createDuel.mutateAsync({
        name,
        pool_id: selectedPool.id,
        white_version_id: duelWhiteId,
        black_version_id: duelBlackId
      });
      setDuelName("");
      if (result.matchId) {
        navigate(`/watch/${encodeURIComponent(result.matchId)}`);
      } else {
        showNotice("Live duel started. Elo will update when the game finishes.");
      }
    } catch (mutationError) {
      showError(mutationError instanceof Error ? mutationError.message : "Request failed");
    }
  }

  return (
    <>
      <section className="panel">
        <div className="panel-header">
          <h2>Live Duel</h2>
          <span>{runningMatches.length > 0 ? "Watching enabled" : "Ready to launch"}</span>
        </div>
        <p className="panel-copy">
          Pick any two engines, start one live game immediately, and then open the fullscreen watch page to
          follow it comfortably. The watch view now leans harder into time pressure and makes the finish state
          unmistakable.
        </p>

        <form className="stack" onSubmit={submitLiveDuel}>
          <Field label="Duel name" hint="Optional">
            <input
              value={duelName}
              onChange={(event) => setDuelName(event.target.value)}
              placeholder="Example: MiniMax vs King Safety"
            />
          </Field>
          <div className="two-up">
            <Field label="Chess type">
              <select value={duelVariant} onChange={(event) => setDuelVariant(event.target.value as Variant)} required>
                <option value="">Select chess type</option>
                {variantChoices.map((variant) => (
                  <option key={variant} value={variant}>
                    {formatVariant(variant)}
                  </option>
                ))}
              </select>
            </Field>
            <Field label="Time control">
              <select value={duelTimeControlKey} onChange={(event) => setDuelTimeControlKey(event.target.value)} required>
                <option value="">Select time control</option>
                {timeControlChoices.map((timeControl) => (
                  <option key={timeControlKey(timeControl)} value={timeControlKey(timeControl)}>
                    {formatTimeControl(timeControl)}
                  </option>
                ))}
              </select>
            </Field>
          </div>
          {!selectedPool && duelVariant && duelTimeControlKey ? (
            <div className="result-strip">
              <strong>Unavailable combination</strong>
              <span>This chess type and time control are not registered together yet.</span>
            </div>
          ) : null}
          {selectedPool ? (
            <div className="result-strip">
              <strong>Selected setup</strong>
              <span>
                {formatVariant(selectedPool.variant)} • {formatTimeControl(selectedPool.time_control)}
              </span>
            </div>
          ) : null}
          <div className="two-up">
            <Field label="White engine">
              <select value={duelWhiteId} onChange={(event) => setDuelWhiteId(event.target.value)} required>
                <option value="">Select engine</option>
                {compatibleVersions.map((version) => (
                  <option key={version.id} value={version.id}>
                    {versionNameById[version.id]}
                  </option>
                ))}
              </select>
            </Field>
            <Field label="Black engine">
              <select value={duelBlackId} onChange={(event) => setDuelBlackId(event.target.value)} required>
                <option value="">Select engine</option>
                {compatibleVersions.map((version) => (
                  <option key={version.id} value={version.id}>
                    {versionNameById[version.id]}
                  </option>
                ))}
              </select>
            </Field>
          </div>

          <div className="duel-preview">
            <EngineSideCard side="white" title="White side" name={versionNameById[duelWhiteId] ?? "Choose an engine"} />
            <EngineSideCard side="black" title="Black side" name={versionNameById[duelBlackId] ?? "Choose an engine"} />
          </div>

          <div className="result-strip">
            <strong>What happens next</strong>
            <span>Launch the duel, jump into fullscreen, and follow the clocks, latest move, and clear final result.</span>
          </div>

          <button type="submit" disabled={createDuel.isPending}>Start live duel</button>
        </form>
      </section>

      <section className="panel replay-panel">
        <div className="panel-header">
          <h2>Live Matches</h2>
          <span>{runningMatches.length > 0 ? `${runningMatches.length} running` : "No live match yet"}</span>
        </div>
        <p className="panel-copy">
          Choose a live match and open the dedicated watch page instead of squeezing the board into this control
          screen. Active games now show stronger urgency cues, and finished ones are easier to spot and review.
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
            <button type="button" onClick={() => navigate(`/watch/${encodeURIComponent(resumableMatch.id)}`)}>
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
                  <button type="button" onClick={() => navigate(`/watch/${encodeURIComponent(match.id)}`)}>
                    Watch fullscreen
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </section>
    </>
  );
}
