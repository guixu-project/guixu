/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import { useDatasets, useUnpublish } from "../api";

function formatSize(bytes: number): string {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  if (bytes >= 1e6) return `${(bytes / 1e6).toFixed(1)} MB`;
  if (bytes >= 1e3) return `${(bytes / 1e3).toFixed(1)} KB`;
  return `${bytes} B`;
}

export default function Datasets() {
  const { data: datasets, isLoading, error } = useDatasets(true);
  const unpublish = useUnpublish();

  if (error) return <div className="p-6 text-agentprism-error">Failed to load datasets</div>;
  if (isLoading) return <div className="p-6 text-agentprism-muted-foreground">Loading…</div>;

  return (
    <div className="p-6 space-y-4">
      <h2 className="text-lg font-semibold">Local Datasets</h2>

      {!datasets || datasets.length === 0 ? (
        <p className="text-sm text-agentprism-muted-foreground">No datasets published yet.</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-agentprism-border text-left text-agentprism-muted-foreground text-xs">
                <th className="py-2 pr-4">Title</th>
                <th className="py-2 pr-4">CID</th>
                <th className="py-2 pr-4">Access</th>
                <th className="py-2 pr-4 text-right">Size</th>
                <th className="py-2 pr-4 text-right">Rows</th>
                <th className="py-2 pr-4">Actions</th>
              </tr>
            </thead>
            <tbody>
              {datasets.map((d) => (
                <tr key={d.cid} className="border-b border-agentprism-border/50">
                  <td className="py-2 pr-4">{d.title}</td>
                  <td className="py-2 pr-4 font-mono text-xs truncate max-w-[180px]">{d.cid}</td>
                  <td className="py-2 pr-4 text-xs">{d.access ?? "open"}</td>
                  <td className="py-2 pr-4 text-right tabular-nums">{formatSize(d.size_bytes ?? 0)}</td>
                  <td className="py-2 pr-4 text-right tabular-nums">{d.row_count?.toLocaleString() ?? "—"}</td>
                  <td className="py-2 pr-4">
                    <button
                      onClick={() => unpublish.mutate(d.cid)}
                      disabled={unpublish.isPending}
                      className="text-xs text-agentprism-error hover:underline disabled:opacity-50"
                    >
                      Unpublish
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
