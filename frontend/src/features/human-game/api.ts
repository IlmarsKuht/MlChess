import { useMutation, useQueryClient } from "@tanstack/react-query";

import { createClientActionId } from "../../app/debug";
import { fetchJson } from "../../shared/api/client";
import { arenaQueryKeys } from "../../shared/queries/arena";

export interface StartHumanGameInput {
  name: string;
  pool_id: string;
  engine_version_id: string;
  human_side: "white" | "black" | "random";
}

export function useStartHumanGameMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (input: StartHumanGameInput) =>
      fetchJson<{ match_id: string }>("/human-games", {
        method: "POST",
        debug: { clientActionId: createClientActionId() },
        body: JSON.stringify(input)
      }),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: arenaQueryKeys.matches }),
        queryClient.invalidateQueries({ queryKey: arenaQueryKeys.games }),
        queryClient.invalidateQueries({ queryKey: arenaQueryKeys.humanProfile })
      ]);
    }
  });
}
