import { FormEvent, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";

import { useFlash } from "../../app/providers/FlashProvider";
import { useAgentsQuery, useAgentVersionsQuery, useHumanProfileQuery, usePoolsQuery } from "../../shared/queries/arena";
import { EngineSideCard, Field, RouteErrorState, RouteLoadingState } from "../../shared/ui";
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
  const [humanPoolId, setHumanPoolId] = useState("");
  const [humanEngineId, setHumanEngineId] = useState("");
  const [humanSide, setHumanSide] = useState<"white" | "black" | "random">("random");

  const standardPools = (pools.data ?? []).filter((pool) => pool.variant === "standard");

  useEffect(() => {
    if (!humanPoolId && standardPools[0]) {
      setHumanPoolId(standardPools[0].id);
    }
  }, [humanPoolId, standardPools]);

  useEffect(() => {
    if (!humanEngineId && versions.data?.[0]) {
      setHumanEngineId(versions.data[0].id);
    }
  }, [humanEngineId, versions.data]);

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

    if (!humanPoolId || !humanEngineId) {
      showError("Pick a standard format and an engine first.");
      return;
    }

    const engineName = versionNameById[humanEngineId] ?? "Engine";
    const chosenName = humanGameName.trim() || `You vs ${engineName}`;

    try {
      const response = await startHumanGame.mutateAsync({
        name: chosenName,
        pool_id: humanPoolId,
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
        Pick any engine, choose your side, and play a live standard game that updates both your Elo and the
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
              {(versions.data ?? []).map((version) => (
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
