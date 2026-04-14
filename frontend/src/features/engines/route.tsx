import { useState } from "react";
import { useNavigate, useParams } from "react-router-dom";

import { useAgentVersionsQuery, useAgentsQuery, useEventPresetsQuery, useLeaderboardQuery, usePoolsQuery } from "../../shared/queries/arena";
import { formatTimeControl, formatVariant } from "../../shared/lib/format";
import { EmptyState, EngineDocumentation, RouteErrorState, RouteLoadingState } from "../../shared/ui";

export function EnginesPage() {
  const navigate = useNavigate();
  const [selectedPoolId, setSelectedPoolId] = useState("");
  const agents = useAgentsQuery();
  const versions = useAgentVersionsQuery();
  const pools = usePoolsQuery();
  const eventPresets = useEventPresetsQuery();
  const leaderboard = useLeaderboardQuery(selectedPoolId || undefined);

  if (agents.isLoading || versions.isLoading || pools.isLoading || eventPresets.isLoading || leaderboard.isLoading) {
    return <RouteLoadingState message="Loading engines and formats..." />;
  }

  const error = agents.error ?? versions.error ?? pools.error ?? eventPresets.error ?? leaderboard.error;
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

  return (
    <>
      <section className="panel">
        <div className="panel-header">
          <h2>Available Engines</h2>
          <span>{versions.data?.length ?? 0} ready to play</span>
        </div>
        <p className="panel-copy">Browse the engines currently available for duels and preset events.</p>

        <div className="summary-grid">
          <div className="summary-card">
            <span>Engine lines</span>
            <strong>{agents.data?.length ?? 0}</strong>
            <p>Distinct engine families</p>
          </div>
          <div className="summary-card">
            <span>Playable versions</span>
            <strong>{versions.data?.length ?? 0}</strong>
            <p>Ready to enter</p>
          </div>
          <div className="summary-card">
            <span>Preset events</span>
            <strong>{eventPresets.data?.length ?? 0}</strong>
            <p>Backend-defined only</p>
          </div>
        </div>

        <div className="section-heading">Engines</div>
        {(versions.data?.length ?? 0) === 0 ? (
          <EmptyState>No engine versions are available right now.</EmptyState>
        ) : (
          <div className="table engine-directory-list">
            {versions.data?.map((version) => (
              <button
                type="button"
                className="table-row table-row-stack replay-row"
                key={version.id}
                onClick={() => navigate(`/engine/${encodeURIComponent(version.id)}`)}
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
            {pools.data?.map((pool) => (
              <option key={pool.id} value={pool.id}>
                {pool.name}
              </option>
            ))}
          </select>
        </div>
        <p className="panel-copy">Browse the formats available for events.</p>

        <div className="section-heading">Available formats</div>
        {(pools.data?.length ?? 0) === 0 ? (
          <EmptyState>No formats are available right now.</EmptyState>
        ) : (
          <div className="table">
            {pools.data?.map((pool) => (
              <div className="table-row table-row-stack" key={pool.id}>
                <div>
                  <strong>{pool.name}</strong>
                  <p>
                    {formatVariant(pool.variant)} • {formatTimeControl(pool.time_control)}
                  </p>
                  <p>{pool.fairness.swap_colors ? "Colors swap between paired games." : "Colors stay as scheduled."}</p>
                  {pool.description ? <p>{pool.description}</p> : null}
                </div>
                <div className="chip">{selectedPoolId === pool.id ? "Selected" : "Format"}</div>
              </div>
            ))}
          </div>
        )}
      </section>
    </>
  );
}

export function EngineDetailPage() {
  const navigate = useNavigate();
  const { engineId = "" } = useParams();
  const agents = useAgentsQuery();
  const versions = useAgentVersionsQuery();

  if (agents.isLoading || versions.isLoading) {
    return <RouteLoadingState message="Loading engine page..." />;
  }

  const error = agents.error ?? versions.error;
  if (error) {
    return <RouteErrorState message={error.message} />;
  }

  const selectedEngineVersion = versions.data?.find((version) => version.id === engineId) ?? null;
  const agentNameById = Object.fromEntries((agents.data ?? []).map((agent) => [agent.id, agent.name]));
  const versionNameById = Object.fromEntries(
    (versions.data ?? []).map((version) => [
      version.id,
      `${version.declared_name ?? agentNameById[version.agent_id] ?? "Engine"} ${version.version}`
    ])
  );

  return (
    <section className="panel engine-page">
      <div className="panel-header">
        <h2>Engine Page</h2>
        <button type="button" className="button-ghost compact-button" onClick={() => navigate("/setup")}>
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
  );
}
