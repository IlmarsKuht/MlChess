import { FormEvent, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";

import { useFlash } from "../../app/providers/FlashProvider";
import type { Variant } from "../../app/types";
import { useAgentsQuery, useAgentVersionsQuery, useHumanProfileQuery, usePoolsQuery } from "../../shared/queries/arena";
import { EngineSideCard, Field, RouteErrorState, RouteLoadingState } from "../../shared/ui";
import { formatTimeControl, formatVariant } from "../../shared/lib/format";
import { findPoolForChoices, timeControlKey, uniquePoolTimeControls, uniquePoolVariants } from "../../shared/lib/pools";
import { supportsVariant } from "../../shared/lib/variants";
import { useStartHumanGameMutation } from "./api";

export function HumanGamePage() {
  const navigate = useNavigate();
  const { showError } = useFlash();
  const agents = useAgentsQuery();
  const versions = useAgentVersionsQuery();
  const pools = usePoolsQuery();
  const humanProfile = useHumanProfileQuery();
  const startHumanGame = useStartHumanGameMutation();
  const [humanGameName, setHumanGameName] = useState("");
  const [humanVariant, setHumanVariant] = useState<Variant | "">("");
  const [humanTimeControlKey, setHumanTimeControlKey] = useState("");
  const [humanEngineId, setHumanEngineId] = useState("");
  const [humanSide, setHumanSide] = useState<"white" | "black" | "random">("random");

  const playablePools = pools.data ?? [];
  const variantChoices = uniquePoolVariants(playablePools);
  const timeControlChoices = uniquePoolTimeControls(playablePools);
  const selectedPool = humanVariant ? findPoolForChoices(playablePools, humanVariant, humanTimeControlKey) : null;
  const compatibleVersions = humanVariant
    ? (versions.data ?? []).filter((version) => supportsVariant(version, humanVariant))
    : (versions.data ?? []);

  useEffect(() => {
    if (!humanVariant && variantChoices[0]) {
      setHumanVariant(variantChoices[0]);
    }
  }, [humanVariant, variantChoices]);

  useEffect(() => {
    if (!humanTimeControlKey && timeControlChoices[0]) {
      setHumanTimeControlKey(timeControlKey(timeControlChoices[0]));
    }
  }, [humanTimeControlKey, timeControlChoices]);

  useEffect(() => {
    if (humanEngineId && selectedPool) {
      const currentVersion = versions.data?.find((version) => version.id === humanEngineId);
      if (currentVersion && supportsVariant(currentVersion, selectedPool.variant)) {
        return;
      }
      setHumanEngineId("");
      return;
    }
    if (!humanEngineId && compatibleVersions[0]) {
      setHumanEngineId(compatibleVersions[0].id);
    }
  }, [compatibleVersions, humanEngineId, selectedPool, versions.data]);

  if (agents.isLoading || versions.isLoading || pools.isLoading || humanProfile.isLoading) {
    return <RouteLoadingState message="Loading human game setup..." />;
  }

  const error = agents.error ?? versions.error ?? pools.error ?? humanProfile.error;
  if (error) {
    return <RouteErrorState message={error.message} />;
  }

  const agentNameById = Object.fromEntries((agents.data ?? []).map((agent) => [agent.id, agent.name]));
  const versionNameById = Object.fromEntries(
    (versions.data ?? []).map((version) => [
      version.id,
      `${version.declared_name ?? agentNameById[version.agent_id] ?? "Engine"} ${version.version}`
    ])
  );

  async function submitHumanGame(event: FormEvent) {
    event.preventDefault();

    if (!selectedPool || !humanEngineId) {
      showError("Pick a chess type, time control, and compatible engine first.");
      return;
    }

    const engineName = versionNameById[humanEngineId] ?? "Engine";
    const chosenName = humanGameName.trim() || `You vs ${engineName}`;

    try {
      const response = await startHumanGame.mutateAsync({
        name: chosenName,
        pool_id: selectedPool.id,
        engine_version_id: humanEngineId,
        human_side: humanSide
      });
      setHumanGameName("");
      navigate(`/watch/${encodeURIComponent(response.match_id)}`);
    } catch (mutationError) {
      showError(mutationError instanceof Error ? mutationError.message : "Request failed");
    }
  }

  return (
    <section className="panel">
      <div className="panel-header">
        <h2>Play vs Engine</h2>
        <span>{humanProfile.data ? `Your Elo ${humanProfile.data.rating.toFixed(1)}` : "Ready to play"}</span>
      </div>
      <p className="panel-copy">
        Pick any compatible engine, choose your side, and play a live game that updates both your Elo and the
        engine&apos;s Elo. After launch you&apos;ll land on the fullscreen board with clocks, move list, and a clearer
        end-of-game result screen.
      </p>

      <form className="stack" onSubmit={submitHumanGame}>
        <Field label="Game name" hint="Optional">
          <input
            value={humanGameName}
            onChange={(event) => setHumanGameName(event.target.value)}
            placeholder="Example: Me vs MiniMax"
          />
        </Field>
        <div className="two-up">
          <Field label="Chess type">
            <select value={humanVariant} onChange={(event) => setHumanVariant(event.target.value as Variant)} required>
              <option value="">Select chess type</option>
              {variantChoices.map((variant) => (
                <option key={variant} value={variant}>
                  {formatVariant(variant)}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Time control">
            <select value={humanTimeControlKey} onChange={(event) => setHumanTimeControlKey(event.target.value)} required>
              <option value="">Select time control</option>
              {timeControlChoices.map((timeControl) => (
                <option key={timeControlKey(timeControl)} value={timeControlKey(timeControl)}>
                  {formatTimeControl(timeControl)}
                </option>
              ))}
            </select>
          </Field>
        </div>
        {!selectedPool && humanVariant && humanTimeControlKey ? (
          <div className="result-strip">
            <strong>Unavailable combination</strong>
            <span>
              This chess type and time control are not registered together yet.
            </span>
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
          <Field label="Engine">
            <select value={humanEngineId} onChange={(event) => setHumanEngineId(event.target.value)} required>
              <option value="">Select engine</option>
              {compatibleVersions.map((version) => (
                <option key={version.id} value={version.id}>
                  {versionNameById[version.id]}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Your side">
            <select value={humanSide} onChange={(event) => setHumanSide(event.target.value as "white" | "black" | "random")}>
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
          <strong>What happens next</strong>
          <span>Start the game, play on the big board, and get an obvious winner or draw screen when it ends.</span>
        </div>

        <div className="result-strip">
          <strong>Human profile</strong>
          <span>
            {humanProfile.data
              ? `${humanProfile.data.games_played} games • ${humanProfile.data.wins}W ${humanProfile.data.draws}D ${humanProfile.data.losses}L`
              : "Your profile is ready."}
          </span>
        </div>

        <button type="submit" disabled={startHumanGame.isPending}>Start human game</button>
      </form>
    </section>
  );
}
