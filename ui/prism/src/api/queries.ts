/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { apiFetch, mcpToolCall } from "./client";
import { queryKeys } from "./keys";
import type {
  NodeStatus,
  DatasetMeta,
  PeersResponse,
  NatInfo,
  MarketSearchResponse,
  MarketPreview,
  TraceSummary,
  SpanRecord,
  QueryProfile,
  WalletBalance,
  WalletTransaction,
  Approval,
  TraceScore,
  MemoryEntry,
} from "./types";

// --- Node ---
export function useNodeStatus() {
  return useQuery({
    queryKey: queryKeys.node.status(),
    queryFn: () => apiFetch<NodeStatus>("/api/node/status"),
  });
}

// --- Datasets ---
export function useDatasets(mine = true) {
  return useQuery({
    queryKey: queryKeys.datasets.list(mine),
    queryFn: () =>
      apiFetch<DatasetMeta[]>(`/api/datasets${mine ? "?mine=true" : ""}`),
  });
}

export function useUnpublish() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (cid: string) =>
      apiFetch<void>(`/api/unpublish/${cid}`, { method: "DELETE" }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["datasets"] }),
  });
}

export function usePublish() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (form: FormData) =>
      apiFetch<{ cid: string }>("/api/publish", { method: "POST", body: form }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["datasets"] }),
  });
}

// --- Network ---
export function useNetworkPeers() {
  return useQuery({
    queryKey: queryKeys.network.peers(),
    queryFn: () => apiFetch<PeersResponse>("/api/network/peers"),
  });
}

export function useNetworkNat() {
  return useQuery({
    queryKey: queryKeys.network.nat(),
    queryFn: () => apiFetch<NatInfo>("/api/network/nat"),
  });
}

// --- Market ---
export function useMarketSearch(q: string) {
  return useQuery({
    queryKey: queryKeys.market.search(q),
    queryFn: () =>
      apiFetch<MarketSearchResponse>(
        `/api/market/search?q=${encodeURIComponent(q)}&limit=20`,
      ),
    enabled: q.trim().length > 0,
  });
}

export function useMarketPreview(cid: string | null) {
  return useQuery({
    queryKey: queryKeys.market.preview(cid ?? ""),
    queryFn: () => apiFetch<MarketPreview>(`/api/market/${cid}/preview?rows=10`),
    enabled: cid !== null,
  });
}

// --- Traces ---
export function useTraces(source = "guixu", limit = 50) {
  return useQuery({
    queryKey: queryKeys.traces.list(source, limit),
    queryFn: () =>
      apiFetch<TraceSummary[]>(`/api/traces?source=${source}&limit=${limit}`),
  });
}

export function useTraceSpans(traceId: string, source = "guixu") {
  return useQuery({
    queryKey: queryKeys.traces.spans(traceId, source),
    queryFn: () =>
      apiFetch<SpanRecord[]>(`/api/traces/${traceId}/spans?source=${source}`),
    enabled: traceId.length > 0,
  });
}

export function useTraceScores(traceId: string) {
  return useQuery({
    queryKey: queryKeys.traceScores.get(traceId),
    queryFn: () => apiFetch<TraceScore[]>(`/api/traces/${traceId}/scores`),
    enabled: traceId.length > 0,
  });
}

// --- Memory ---
export function useMemoryTimeline(key?: string, limit = 50) {
  return useQuery({
    queryKey: queryKeys.memory.timeline(key),
    queryFn: () =>
      apiFetch<MemoryEntry[]>(
        `/api/memory/timeline?${key ? `memory_key=${encodeURIComponent(key)}&` : ""}limit=${limit}`,
      ),
  });
}

// --- Wallet ---
export function useWalletBalance() {
  return useQuery({
    queryKey: queryKeys.wallet.balance(),
    queryFn: () => apiFetch<WalletBalance>("/api/wallet/balance"),
  });
}

export function useWalletTransactions() {
  return useQuery({
    queryKey: queryKeys.wallet.transactions(),
    queryFn: () => apiFetch<WalletTransaction[]>("/api/wallet/transactions"),
  });
}

// --- Approvals (via MCP tools) ---
export function useApprovals() {
  return useQuery({
    queryKey: queryKeys.approvals.list(),
    queryFn: () => mcpToolCall<{ tasks: Approval[] }>("data_task_status", {}),
    select: (data) => data.tasks ?? [],
  });
}

export function useApproveTask() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (args: { task_id: string; decision: "approve" | "reject" }) =>
      mcpToolCall("data_task_approve", args),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["approvals"] }),
  });
}

// --- MCP Tool Mutations ---
export function useIntentParse() {
  return useMutation({
    mutationFn: (profile: QueryProfile) =>
      mcpToolCall<QueryProfile>("intent_parse", profile as unknown as Record<string, unknown>),
  });
}

export function useMcpSearch() {
  return useMutation({
    mutationFn: (args: { query: string; task_type?: string; filters?: Record<string, unknown> }) =>
      mcpToolCall<{ results: MarketSearchResponse["results"] }>("dataset_search", args),
  });
}

export function useMcpEvaluate() {
  return useMutation({
    mutationFn: (args: { cid: string; task_description: string; task_type?: string }) =>
      mcpToolCall<Record<string, unknown>>("dataset_evaluate", args),
  });
}
