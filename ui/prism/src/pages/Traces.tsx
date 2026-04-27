/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import { useState } from "react";
import { useQueries } from "@tanstack/react-query";
import type { TraceSpan, TraceRecord } from "@evilmartians/agent-prism-types";
import {
  TraceViewer,
  type TraceViewerData,
} from "../vendor/ui-components/TraceViewer/TraceViewer";
import { fetchSpans } from "../guixu-adapter";
import { useTraces, useTraceScores, useMemoryTimeline } from "../api";
import { queryKeys } from "../api/keys";
import type { TraceSummary, TraceScore, MemoryEntry } from "../api/types";

// --- Scores Overlay ---

function ScoresOverlay({ traceId }: { traceId: string }) {
  const { data: scores, isLoading } = useTraceScores(traceId);
  if (isLoading || !scores || scores.length === 0) return null;

  return (
    <div className="flex flex-wrap gap-2 p-3 border-b border-agentprism-border">
      {scores.map((s, i) => (
        <div key={i} className="rounded border border-agentprism-border bg-agentprism-card px-3 py-1.5">
          <span className="text-[10px] text-agentprism-muted-foreground">{s.metric}</span>
          <span className="ml-2 text-sm font-bold">{s.value}</span>
          {s.label && <span className="ml-1 text-[10px] text-agentprism-muted-foreground">{s.label}</span>}
        </div>
      ))}
    </div>
  );
}

// --- Memory Timeline ---

function MemoryTimeline() {
  const { data: entries, isLoading } = useMemoryTimeline(undefined, 30);

  if (isLoading) return <p className="text-xs text-agentprism-muted-foreground p-3">Loading memory…</p>;
  if (!entries || entries.length === 0) return <p className="text-xs text-agentprism-muted-foreground p-3">No memory mutations recorded.</p>;

  return (
    <div className="p-3 space-y-1 max-h-[200px] overflow-y-auto">
      <h4 className="text-xs font-semibold text-agentprism-muted-foreground mb-1">Memory Timeline</h4>
      {entries.map((e, i) => (
        <div key={i} className="flex items-start gap-2 text-[10px]">
          <span className="text-agentprism-muted-foreground shrink-0 tabular-nums">
            {new Date(e.timestamp).toLocaleTimeString()}
          </span>
          <span className={`shrink-0 rounded px-1 py-0.5 ${e.operation === "set" ? "bg-emerald-500/10 text-emerald-400" : e.operation === "delete" ? "bg-red-500/10 text-red-400" : "bg-amber-500/10 text-amber-400"}`}>
            {e.operation}
          </span>
          <span className="font-mono">{e.memory_key}</span>
          {e.value_summary && <span className="text-agentprism-muted-foreground truncate">{e.value_summary}</span>}
        </div>
      ))}
    </div>
  );
}

// --- Main Traces Page ---

export default function TracesPage() {
  const { data: traceSummaries, isLoading, error } = useTraces();
  const [selectedTraceId, setSelectedTraceId] = useState<string | null>(null);

  const spanQueries = useQueries({
    queries: (traceSummaries ?? []).map((t) => ({
      queryKey: queryKeys.traces.spans(t.trace_id, "guixu"),
      queryFn: () => fetchSpans(t.trace_id),
    })),
  });

  const allSpansLoaded = spanQueries.every((q) => !q.isLoading);

  if (isLoading)
    return <div className="flex items-center justify-center h-full"><p className="text-agentprism-muted-foreground">Loading traces…</p></div>;
  if (error)
    return <div className="flex items-center justify-center h-full"><p className="text-agentprism-error">{error instanceof Error ? error.message : "Failed to load traces"}</p></div>;
  if (!traceSummaries || traceSummaries.length === 0)
    return <div className="flex items-center justify-center h-full"><p className="text-agentprism-muted-foreground">No traces found. Run some agent workflows first.</p></div>;
  if (!allSpansLoaded)
    return <div className="flex items-center justify-center h-full"><p className="text-agentprism-muted-foreground">Loading spans…</p></div>;

  const data: TraceViewerData[] = traceSummaries.map((t, i) => ({
    traceRecord: {
      id: t.trace_id,
      name: t.trace_name ?? t.trace_id.slice(0, 8),
      spansCount: t.span_count,
      durationMs: t.total_duration_ms,
      agentDescription: `${t.source} trace`,
      totalTokens: t.total_input_tokens + t.total_output_tokens || undefined,
      startTime: new Date(t.first_span_time).getTime(),
    } satisfies TraceRecord,
    spans: (spanQueries[i]?.data as TraceSpan[]) ?? [],
  }));

  // Auto-select first trace if none selected
  const activeTraceId = selectedTraceId ?? traceSummaries[0]?.trace_id ?? "";

  return (
    <div className="flex flex-col h-full">
      {/* Scores overlay for selected trace */}
      {activeTraceId && <ScoresOverlay traceId={activeTraceId} />}

      {/* Trace viewer */}
      <div className="flex-1 min-h-0">
        <TraceViewer data={data} />
      </div>

      {/* Memory timeline */}
      <div className="border-t border-agentprism-border shrink-0">
        <MemoryTimeline />
      </div>
    </div>
  );
}
