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
import Dashboard from "./pages/Dashboard";
import Network from "./pages/Network";
import Market from "./pages/Market";

import "./vendor/ui-components/theme/theme.css";
import "./vendor/ui-index.css";

type Page = "dashboard" | "traces" | "network" | "market";
type SpansCache = Record<string, TraceSpan[]>;

const NAV_ITEMS: { page: Page; label: string }[] = [
  { page: "dashboard", label: "Dashboard" },
  { page: "network", label: "Network" },
  { page: "market", label: "Market" },
  { page: "traces", label: "Traces" },
];

function getPageFromHash(): Page {
  const hash = window.location.hash.replace("#", "");
  if (NAV_ITEMS.some((n) => n.page === hash)) return hash as Page;
  return "dashboard";
}

function TracesPage() {
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

  if (loading)
    return (
      <div className="flex items-center justify-center h-full">
        <p className="text-agentprism-muted-foreground">Loading traces…</p>
      </div>
    );
  if (error)
    return (
      <div className="flex flex-col items-center justify-center h-full gap-4">
        <p className="text-agentprism-error">{error}</p>
        <button
          onClick={loadTraces}
          className="bg-agentprism-primary text-agentprism-primary-foreground rounded px-4 py-2"
        >
          Retry
        </button>
      </div>
    );
  if (traces.length === 0)
    return (
      <div className="flex items-center justify-center h-full">
        <p className="text-agentprism-muted-foreground">
          No traces found. Run some agent workflows first.
        </p>
      </div>
    );

  const data: TraceViewerData[] = traces.map((t) => ({
    traceRecord: t,
    spans: spansCache[t.id] ?? [],
  }));

  return <TraceViewer data={data} />;
}

export default function App() {
  const [page, setPage] = useState<Page>(getPageFromHash);

  useEffect(() => {
    const onHash = () => setPage(getPageFromHash());
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);

  const navigate = (p: Page) => {
    window.location.hash = p;
  };

  return (
    <div className="bg-agentprism-background text-agentprism-foreground h-screen flex flex-col">
      <nav className="flex items-center h-[50px] border-b border-agentprism-border px-4 gap-6 shrink-0">
        <h1 className="text-sm font-semibold mr-4">Guixu</h1>
        {NAV_ITEMS.map((n) => (
          <button
            key={n.page}
            onClick={() => navigate(n.page)}
            className={`text-sm py-1 border-b-2 transition-colors ${
              page === n.page
                ? "border-agentprism-primary text-agentprism-foreground font-medium"
                : "border-transparent text-agentprism-muted-foreground hover:text-agentprism-foreground"
            }`}
          >
            {n.label}
          </button>
        ))}
      </nav>
      <main className="flex-1 overflow-auto">
        {page === "dashboard" && <Dashboard />}
        {page === "network" && <Network />}
        {page === "market" && <Market />}
        {page === "traces" && <TracesPage />}
      </main>
    </div>
  );
}
