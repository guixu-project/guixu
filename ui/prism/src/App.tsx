/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import { useCallback, useEffect, useState } from "react";
import type { TraceSpan, TraceRecord } from "@evilmartians/agent-prism-types";
import {
  TraceViewer,
  type TraceViewerData,
} from "./vendor/ui-components/TraceViewer/TraceViewer";
import { fetchTraces, fetchSpans } from "./guixu-adapter";

import "./vendor/ui-components/theme/theme.css";
import "./vendor/ui-index.css";

type SpansCache = Record<string, TraceSpan[]>;

export default function App() {
  const [traces, setTraces] = useState<TraceRecord[]>([]);
  const [spansCache, setSpansCache] = useState<SpansCache>({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loadTraces = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const records = await fetchTraces();
      setTraces(records);
      // Pre-load spans for all traces
      const cache: SpansCache = {};
      await Promise.all(
        records.map(async (r) => {
          try {
            cache[r.id] = await fetchSpans(r.id);
          } catch {
            cache[r.id] = [];
          }
        }),
      );
      setSpansCache(cache);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load traces");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadTraces();
  }, [loadTraces]);

  if (loading) {
    return (
      <div className="bg-agentprism-background text-agentprism-foreground flex h-screen items-center justify-center">
        <p className="text-agentprism-muted-foreground">Loading traces…</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className="bg-agentprism-background text-agentprism-foreground flex h-screen flex-col items-center justify-center gap-4">
        <p className="text-agentprism-error">{error}</p>
        <button
          onClick={loadTraces}
          className="bg-agentprism-primary text-agentprism-primary-foreground rounded px-4 py-2"
        >
          Retry
        </button>
      </div>
    );
  }

  if (traces.length === 0) {
    return (
      <div className="bg-agentprism-background text-agentprism-foreground flex h-screen items-center justify-center">
        <p className="text-agentprism-muted-foreground">
          No traces found. Run some agent workflows first.
        </p>
      </div>
    );
  }

  const data: TraceViewerData[] = traces.map((t) => ({
    traceRecord: t,
    spans: spansCache[t.id] ?? [],
  }));

  return (
    <div className="bg-agentprism-background text-agentprism-foreground h-screen">
      <div className="flex h-[50px] items-center border-b border-agentprism-border px-4">
        <h1 className="text-sm font-semibold">Guixu Trace Viewer</h1>
      </div>
      <TraceViewer data={data} />
    </div>
  );
}
