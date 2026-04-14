import { useMemo } from "react";
import { useQueries, useQuery } from "@tanstack/react-query";

import { fetchJson } from "../api/client";
import type {
  Agent,
  AgentVersion,
  BenchmarkPool,
  EventPreset,
  GameRecord,
  HumanPlayerProfile,
  LeaderboardEntry,
  MatchSeries,
  Tournament
} from "../../app/types";

export const arenaQueryKeys = {
  agents: ["agents"] as const,
  agentVersions: (scope: string) => ["agentVersions", scope] as const,
  pools: ["pools"] as const,
  eventPresets: ["eventPresets"] as const,
  tournaments: ["tournaments"] as const,
  matches: ["matches"] as const,
  games: ["games"] as const,
  leaderboard: (poolId?: string) => ["leaderboard", poolId ?? "all"] as const,
  humanProfile: ["humanProfile"] as const,
  replay: (gameId: string) => ["replay", gameId] as const,
  tournamentMatches: (tournamentId: string) => ["tournamentMatches", tournamentId] as const
};

export function useAgentsQuery(refetchInterval?: number) {
  return useQuery({
    queryKey: arenaQueryKeys.agents,
    queryFn: () => fetchJson<Agent[]>("/agents"),
    refetchInterval
  });
}

export function useAgentVersionsQuery(agentIds?: string[]) {
  const agentsQuery = useAgentsQuery();
  const resolvedIds = agentIds ?? agentsQuery.data?.map((agent) => agent.id) ?? [];
  const scope = agentIds ? agentIds.join(",") : "all";
  const query = useQuery({
    queryKey: arenaQueryKeys.agentVersions(scope),
    enabled: resolvedIds.length > 0,
    queryFn: async () => {
      const responses = await Promise.all(
        resolvedIds.map((agentId) => fetchJson<AgentVersion[]>(`/agents/${agentId}/versions`))
      );
      return responses.flat();
    }
  });

  return {
    ...query,
    isLoading: agentsQuery.isLoading || query.isLoading,
    error: (query.error ?? agentsQuery.error) as Error | null
  };
}

export function usePoolsQuery(refetchInterval?: number) {
  return useQuery({
    queryKey: arenaQueryKeys.pools,
    queryFn: () => fetchJson<BenchmarkPool[]>("/pools"),
    refetchInterval
  });
}

export function useEventPresetsQuery(refetchInterval?: number) {
  return useQuery({
    queryKey: arenaQueryKeys.eventPresets,
    queryFn: () => fetchJson<EventPreset[]>("/event-presets"),
    refetchInterval
  });
}

export function useTournamentsQuery(refetchInterval?: number) {
  return useQuery({
    queryKey: arenaQueryKeys.tournaments,
    queryFn: () => fetchJson<Tournament[]>("/tournaments"),
    refetchInterval
  });
}

export function useMatchesQuery(refetchInterval?: number) {
  return useQuery({
    queryKey: arenaQueryKeys.matches,
    queryFn: () => fetchJson<MatchSeries[]>("/matches"),
    refetchInterval
  });
}

export function useGamesQuery(refetchInterval?: number) {
  return useQuery({
    queryKey: arenaQueryKeys.games,
    queryFn: () => fetchJson<GameRecord[]>("/games"),
    refetchInterval
  });
}

export function useLeaderboardQuery(poolId?: string, refetchInterval?: number) {
  return useQuery({
    queryKey: arenaQueryKeys.leaderboard(poolId),
    queryFn: () =>
      fetchJson<LeaderboardEntry[]>(poolId ? `/leaderboards?pool_id=${encodeURIComponent(poolId)}` : "/leaderboards"),
    refetchInterval
  });
}

export function useHumanProfileQuery() {
  return useQuery({
    queryKey: arenaQueryKeys.humanProfile,
    queryFn: () => fetchJson<HumanPlayerProfile>("/human-player")
  });
}

export function useArenaSummaryQueries(active = true) {
  const interval = active ? 3000 : false;
  const agents = useAgentsQuery();
  const versions = useAgentVersionsQuery();
  const pools = usePoolsQuery(active ? 3000 : undefined);
  const tournaments = useTournamentsQuery(active ? 3000 : undefined);
  const matches = useMatchesQuery(active ? 3000 : undefined);
  const games = useGamesQuery(active ? 3000 : undefined);
  const eventPresets = useEventPresetsQuery(active ? 3000 : undefined);

  return { agents, versions, pools, tournaments, matches, games, eventPresets, interval };
}

export function useTournamentMatchesQuery(tournamentId: string, enabled = true) {
  return useQuery({
    queryKey: arenaQueryKeys.tournamentMatches(tournamentId),
    enabled,
    queryFn: () => fetchJson<MatchSeries[]>(`/matches?tournament_id=${encodeURIComponent(tournamentId)}`)
  });
}
