import { useMutation, useQueryClient } from "@tanstack/react-query";

import { createClientActionId } from "../../app/debug";
import type { MatchSeries } from "../../app/types";
import { fetchJson } from "../../shared/api/client";
import { arenaQueryKeys } from "../../shared/queries/arena";

export interface CreateLiveDuelInput {
  name: string;
  pool_id: string;
  white_version_id: string;
  black_version_id: string;
}

async function waitForTournamentMatch(tournamentId: string) {
  const deadline = Date.now() + 8000;

  while (Date.now() < deadline) {
    const tournamentMatches = await fetchJson<MatchSeries[]>(
      `/matches?tournament_id=${encodeURIComponent(tournamentId)}`
    );
    const liveMatch = tournamentMatches.find((match) => match.status === "running" && match.watch_state === "live");
    if (liveMatch) {
      return liveMatch.id;
    }

    await new Promise<void>((resolve) => {
      window.setTimeout(resolve, 250);
    });
  }

  return "";
}

export function useCreateLiveDuelMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (input: CreateLiveDuelInput) => {
      const response = await fetchJson<{ tournament_id: string }>("/duels", {
        method: "POST",
        debug: { clientActionId: createClientActionId() },
        body: JSON.stringify(input)
      });
      return {
        tournamentId: response.tournament_id,
        matchId: await waitForTournamentMatch(response.tournament_id)
      };
    },
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: arenaQueryKeys.tournaments }),
        queryClient.invalidateQueries({ queryKey: arenaQueryKeys.matches }),
        queryClient.invalidateQueries({ queryKey: arenaQueryKeys.games }),
        queryClient.invalidateQueries({ queryKey: ["leaderboard"] })
      ]);
    }
  });
}
