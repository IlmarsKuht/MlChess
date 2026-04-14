import { useQuery } from "@tanstack/react-query";

import type { ReplayPayload } from "../../app/types";
import { arenaQueryKeys } from "../../shared/queries/arena";
import { fetchJson } from "../../shared/api/client";

export function useReplayQuery(gameId: string) {
  return useQuery({
    queryKey: arenaQueryKeys.replay(gameId),
    enabled: Boolean(gameId),
    queryFn: () => fetchJson<ReplayPayload>(`/games/${gameId}/replay`)
  });
}
