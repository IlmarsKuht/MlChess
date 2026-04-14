import { useNavigate } from "react-router-dom";

import { useFlash } from "../../app/providers/FlashProvider";
import { useStartEventPresetMutation } from "./api";
import { formatTournamentKind } from "../../shared/lib/format";
import { useEventPresetsQuery, usePoolsQuery } from "../../shared/queries/arena";
import { EmptyState, RouteErrorState, RouteLoadingState } from "../../shared/ui";

export function EventsPage() {
  const navigate = useNavigate();
  const { showError, showNotice } = useFlash();
  const presets = useEventPresetsQuery(3000);
  const pools = usePoolsQuery(3000);
  const startEvent = useStartEventPresetMutation();

  if (presets.isLoading || pools.isLoading) {
    return <RouteLoadingState message="Loading event presets..." />;
  }

  const error = presets.error ?? pools.error;
  if (error) {
    return <RouteErrorState message={error.message} />;
  }

  const poolNameById = Object.fromEntries((pools.data ?? []).map((pool) => [pool.id, pool.name]));

  return (
    <section className="panel">
      <div className="panel-header">
        <h2>Event Presets</h2>
        <span>{startEvent.isPending ? "Refreshing..." : `${presets.data?.length ?? 0} ready`}</span>
      </div>
      <p className="panel-copy">
        These events are defined in backend setup. Starting one launches a fresh run with the current active engine
        lineup.
      </p>

      <div className="section-heading">Available events</div>
      {(presets.data?.length ?? 0) === 0 ? (
        <EmptyState>No backend-defined event presets are available right now.</EmptyState>
      ) : (
        <div className="table">
          {presets.data?.map((preset) => (
            <div className="table-row" key={preset.id}>
              <div>
                <strong>{preset.name}</strong>
                <p>
                  {formatTournamentKind(preset.kind)} • {poolNameById[preset.pool_id] ?? "Unknown format"} •{" "}
                  {preset.games_per_pairing} game{preset.games_per_pairing === 1 ? "" : "s"} per pairing
                </p>
                <p>Selection mode: all active engines • Workers: {preset.worker_count}</p>
              </div>
              <button
                type="button"
                disabled={!preset.active || startEvent.isPending}
                onClick={async () => {
                  try {
                    await startEvent.mutateAsync(preset.id);
                    showNotice("Event started. Live results refresh automatically.");
                    navigate("/events");
                  } catch (mutationError) {
                    showError(mutationError instanceof Error ? mutationError.message : "Request failed");
                  }
                }}
              >
                {preset.active ? "Start" : "Inactive"}
              </button>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
