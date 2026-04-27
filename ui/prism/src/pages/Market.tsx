/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import { useState } from "react";
import { useMarketSearch, useMarketPreview } from "../api";

function formatSize(bytes: number): string {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  if (bytes >= 1e6) return `${(bytes / 1e6).toFixed(1)} MB`;
  if (bytes >= 1e3) return `${(bytes / 1e3).toFixed(1)} KB`;
  return `${bytes} B`;
}

export default function Market() {
  const [input, setInput] = useState("");
  const [query, setQuery] = useState("");
  const [previewCid, setPreviewCid] = useState<string | null>(null);

  const { data: searchData, isLoading, isFetched } = useMarketSearch(query);
  const { data: preview } = useMarketPreview(previewCid);

  const results = searchData?.results ?? [];

  const doSearch = () => {
    if (input.trim()) setQuery(input.trim());
  };

  return (
    <div className="p-6 space-y-6">
      <h2 className="text-lg font-semibold">Data Market</h2>

      <div className="flex gap-2">
        <input
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && doSearch()}
          placeholder="Search datasets (e.g. sentiment, finance, images)…"
          className="flex-1 rounded-lg border border-agentprism-border bg-agentprism-card px-3 py-2 text-sm outline-none focus:border-agentprism-primary"
        />
        <button
          onClick={doSearch}
          disabled={isLoading}
          className="rounded-lg bg-agentprism-primary text-agentprism-primary-foreground px-4 py-2 text-sm font-medium disabled:opacity-50"
        >
          {isLoading ? "Searching…" : "Search"}
        </button>
      </div>

      {isFetched && query && results.length === 0 && !isLoading && (
        <p className="text-sm text-agentprism-muted-foreground">
          No datasets found.
        </p>
      )}

      <div className="space-y-3">
        {results.map((r) => {
          const previewText =
            previewCid === r.cid && preview?.schema
              ? `Columns: ${preview.schema.columns.map((c) => `${c.name} (${c.dtype})`).join(", ")}`
              : previewCid === r.cid
                ? "Loading preview…"
                : null;

          return (
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
                  onClick={() =>
                    setPreviewCid(previewCid === r.cid ? null : r.cid)
                  }
                  className="ml-auto text-agentprism-primary hover:underline"
                >
                  {previewCid === r.cid ? "Hide" : "Preview"}
                </button>
              </div>
              {previewText && (
                <pre className="mt-2 rounded bg-agentprism-background p-3 text-xs overflow-x-auto max-h-48 whitespace-pre">
                  {previewText}
                </pre>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
