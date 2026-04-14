import { useMutation, useQueryClient } from "@tanstack/react-query";

import { createClientActionId } from "../../app/debug";
import { fetchJson } from "../../shared/api/client";
import { arenaQueryKeys } from "../../shared/queries/arena";

export function useStartEventPresetMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (id: string) => {
      await fetchJson(`/event-presets/${id}/start`, {
        method: "POST",
        debug: { clientActionId: createClientActionId() }
      });
    },
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: arenaQueryKeys.eventPresets }),
        queryClient.invalidateQueries({ queryKey: arenaQueryKeys.tournaments }),
        queryClient.invalidateQueries({ queryKey: arenaQueryKeys.matches }),
        queryClient.invalidateQueries({ queryKey: arenaQueryKeys.games }),
        queryClient.invalidateQueries({ queryKey: ["leaderboard"] })
      ]);
    }
  });
}
