import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";

export function useAccounts() {
  return useQuery({ queryKey: ["accounts"], queryFn: api.listAccounts });
}

export function useSyncStatus(refetchInterval = 4000) {
  return useQuery({
    queryKey: ["sync", "status"],
    queryFn: api.syncStatus,
    refetchInterval,
  });
}

export function useProviders() {
  return useQuery({
    queryKey: ["providers"],
    queryFn: api.providers,
    staleTime: Infinity,
  });
}
