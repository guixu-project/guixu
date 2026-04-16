/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import { useState } from "react";

interface DatasetResult {
  cid: string;
  title: string;
  description: string | null;
  tags: string[];
  row_count: number;
  size_bytes: number;
  price: { amount: number; currency: string };
  provider: string;
  access: string;
}

function formatSize(bytes: number): string {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  if (bytes >= 1e6) return `${(bytes / 1e6).toFixed(1)} MB`;
  if (bytes >= 1e3) return `${(bytes / 1e3).toFixed(1)} KB`;
  return `${bytes} B`;
}

export default function Market() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<DatasetResult[]>([]);
  const [loading, setLoading] = useState(false);
  const [searched, setSearched] = useState(false);
  const [preview, setPreview] = useState<{
    cid: string;
    data: string;
  } | null>(null);

  const doSearch = async () => {
    if (!query.trim()) return;
    setLoading(true);
    setSearched(true);
    try {
      const res = await fetch(
        `/api/market/search?q=${encodeURIComponent(query)}&limit=20`,
      );
      if (res.ok) {
        const json = await res.json();
        setResults(json.results ?? []);
      } else setResults([]);
    } catch {
      setResults([]);
    } finally {
      setLoading(false);
    }
  };

  const loadPreview = async (cid: string) => {
    try {
      const res = await fetch(`/api/market/${cid}/preview?rows=10`);
      if (res.ok) {
        const data = await res.json();
        // The preview endpoint returns { cid, schema, source }. Display schema columns.
        const previewText = data.schema
          ? `Columns: ${data.schema.columns.map((c: any) => `${c.name} (${c.dtype})`).join(", ")}`
          : "No schema available";
        setPreview({ cid, data: previewText });
      }
    } catch {
      setPreview({ cid, data: "Failed to load preview" });
    }
  };

  return (
    <div className="p-6 space-y-6">
      <h2 className="text-lg font-semibold">Data Market</h2>

      <div className="flex gap-2">
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && doSearch()}
          placeholder="Search datasets (e.g. sentiment, finance, images)…"
          className="flex-1 rounded-lg border border-agentprism-border bg-agentprism-card px-3 py-2 text-sm outline-none focus:border-agentprism-primary"
        />
        <button
          onClick={doSearch}
          disabled={loading}
          className="rounded-lg bg-agentprism-primary text-agentprism-primary-foreground px-4 py-2 text-sm font-medium disabled:opacity-50"
        >
          {loading ? "Searching…" : "Search"}
        </button>
      </div>

      {searched && results.length === 0 && !loading && (
        <p className="text-sm text-agentprism-muted-foreground">
          No datasets found.
        </p>
      )}

      <div className="space-y-3">
        {results.map((r) => (
          <div
            key={r.cid}
            className="rounded-lg border border-agentprism-border bg-agentprism-card p-4 space-y-2"
          >
            <div className="flex items-start justify-between gap-4">
              <div className="min-w-0">
                <h3 className="font-medium truncate">{r.title}</h3>
                {r.description && (
                  <p className="text-xs text-agentprism-muted-foreground mt-0.5 line-clamp-2">
                    {r.description}
                  </p>
                )}
              </div>
              <span className="shrink-0 rounded bg-agentprism-primary/10 text-agentprism-primary px-2 py-0.5 text-xs font-medium">
                {r.price.amount > 0
                  ? `$${r.price.amount.toFixed(2)}`
                  : "Free"}
              </span>
            </div>
            <div className="flex flex-wrap gap-1">
              {r.tags.slice(0, 5).map((t) => (
                <span
                  key={t}
                  className="rounded bg-agentprism-muted px-1.5 py-0.5 text-[10px] text-agentprism-muted-foreground"
                >
                  {t}
                </span>
              ))}
            </div>
            <div className="flex items-center gap-4 text-xs text-agentprism-muted-foreground">
              <span>{r.row_count.toLocaleString()} rows</span>
              <span>{formatSize(r.size_bytes)}</span>
              <span className="font-mono truncate max-w-[120px]">
                {r.cid}
              </span>
              <button
                onClick={() => loadPreview(r.cid)}
                className="ml-auto text-agentprism-primary hover:underline"
              >
                Preview
              </button>
            </div>
            {preview?.cid === r.cid && (
              <pre className="mt-2 rounded bg-agentprism-background p-3 text-xs overflow-x-auto max-h-48 whitespace-pre">
                {preview.data}
              </pre>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
