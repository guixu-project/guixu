/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import type {
  TraceSpan,
  TraceSpanCategory,
  TraceSpanStatus,
  TraceRecord,
} from "@evilmartians/agent-prism-types";

// --- Guixu API response types ---

interface GuixuTraceSummary {
  trace_id: string;
  trace_name: string | null;
  session_id: string | null;
  source: string;
  first_span_time: string;
  last_span_time: string;
  total_duration_ms: number;
  span_count: number;
  total_input_tokens: number;
  total_output_tokens: number;
}

interface GuixuSpanRecord {
  trace_id: string;
  span_id: string;
  parent_span_id: string | null;
  session_id: string | null;
  span_name: string;
  span_type: string;
  source: string;
  start_time: string;
  end_time: string;
  duration_ms: number;
  attributes: Record<string, unknown>;
  input_tokens: number | null;
  output_tokens: number | null;
  model: string | null;
  error: string | null;
}

// --- Mapping helpers ---

const SPAN_TYPE_MAP: Record<string, TraceSpanCategory> = {
  agent: "agent_invocation",
  generation: "llm_call",
  tool_use: "tool_execution",
  guardrail: "guardrail",
  handoff: "chain_operation",
  user: "span",
  system: "span",
  other: "unknown",
  memory_mutation: "event",
};

function mapSpanType(t: string): TraceSpanCategory {
  return SPAN_TYPE_MAP[t] ?? "unknown";
}

function mapStatus(error: string | null): TraceSpanStatus {
  return error ? "error" : "success";
}

function extractIO(attrs: Record<string, unknown>): {
  input?: string;
  output?: string;
} {
  const input =
    typeof attrs["input"] === "string"
      ? attrs["input"]
      : typeof attrs["input.value"] === "string"
        ? attrs["input.value"]
        : undefined;
  const output =
    typeof attrs["output"] === "string"
      ? attrs["output"]
      : typeof attrs["output.value"] === "string"
        ? attrs["output.value"]
        : undefined;
  return { input, output };
}

function convertSpan(s: GuixuSpanRecord): TraceSpan & { _parentId?: string } {
  const io = extractIO(s.attributes);
  return {
    id: s.span_id,
    title: s.span_name,
    startTime: new Date(s.start_time),
    endTime: new Date(s.end_time),
    duration: s.duration_ms,
    type: mapSpanType(s.span_type),
    status: mapStatus(s.error),
    raw: JSON.stringify(s),
    tokensCount:
      s.input_tokens || s.output_tokens
        ? (s.input_tokens ?? 0) + (s.output_tokens ?? 0)
        : undefined,
    input: io.input,
    output: io.output,
    attributes: Object.entries(s.attributes).map(([key, value]) => ({
      key,
      value: { stringValue: String(value) },
    })),
    children: [],
    _parentId: s.parent_span_id ?? undefined,
  };
}

function buildTree(spans: (TraceSpan & { _parentId?: string })[]): TraceSpan[] {
  const map = new Map<string, TraceSpan & { _parentId?: string }>();
  for (const s of spans) map.set(s.id, s);

  const roots: TraceSpan[] = [];
  for (const s of spans) {
    if (s._parentId && map.has(s._parentId)) {
      const parent = map.get(s._parentId)!;
      if (!parent.children) parent.children = [];
      parent.children.push(s);
    } else {
      roots.push(s);
    }
  }

  // Clean up internal field
  for (const s of spans) delete (s as Record<string, unknown>)["_parentId"];
  return roots;
}

// --- Public API ---

const API_BASE = "";

export async function fetchTraces(
  source = "guixu",
  limit = 50,
): Promise<TraceRecord[]> {
  const res = await fetch(
    `${API_BASE}/api/traces?source=${source}&limit=${limit}`,
  );
  if (!res.ok) throw new Error(`Failed to fetch traces: ${res.status}`);
  const data: GuixuTraceSummary[] = await res.json();
  return data.map((t) => ({
    id: t.trace_id,
    name: t.trace_name ?? t.trace_id.slice(0, 8),
    spansCount: t.span_count,
    durationMs: t.total_duration_ms,
    agentDescription: `${t.source} trace`,
    totalTokens: t.total_input_tokens + t.total_output_tokens || undefined,
    startTime: new Date(t.first_span_time).getTime(),
  }));
}

export async function fetchSpans(
  traceId: string,
  source = "guixu",
): Promise<TraceSpan[]> {
  const res = await fetch(
    `${API_BASE}/api/traces/${traceId}/spans?source=${source}`,
  );
  if (!res.ok) throw new Error(`Failed to fetch spans: ${res.status}`);
  const data: GuixuSpanRecord[] = await res.json();
  return buildTree(data.map(convertSpan));
}
