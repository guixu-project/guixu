/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

export const queryKeys = {
  node: {
    status: () => ["node", "status"] as const,
  },
  datasets: {
    list: (mine?: boolean) => ["datasets", { mine }] as const,
    detail: (cid: string) => ["datasets", cid] as const,
  },
  network: {
    peers: () => ["network", "peers"] as const,
    nat: () => ["network", "nat"] as const,
  },
  market: {
    search: (q: string) => ["market", "search", q] as const,
    preview: (cid: string) => ["market", "preview", cid] as const,
  },
  traces: {
    list: (source: string, limit: number) =>
      ["traces", { source, limit }] as const,
    spans: (traceId: string, source: string) =>
      ["traces", traceId, "spans", source] as const,
  },
  wallet: {
    balance: () => ["wallet", "balance"] as const,
    transactions: () => ["wallet", "transactions"] as const,
  },
  approvals: {
    list: () => ["approvals"] as const,
  },
  discover: {
    intentParse: (query: string) => ["discover", "intent", query] as const,
    search: (query: string) => ["discover", "search", query] as const,
    evaluate: (cid: string) => ["discover", "evaluate", cid] as const,
  },
  traceScores: {
    get: (traceId: string) => ["traces", traceId, "scores"] as const,
  },
  memory: {
    timeline: (key?: string) => ["memory", "timeline", key] as const,
  },
} as const;
