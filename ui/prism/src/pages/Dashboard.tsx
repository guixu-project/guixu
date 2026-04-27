/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import { useNodeStatus, useDatasets } from "../api";
import { useNavigate } from "@tanstack/react-router";

function formatSize(bytes: number): string {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  if (bytes >= 1e6) return `${(bytes / 1e6).toFixed(1)} MB`;
  if (bytes >= 1e3) return `${(bytes / 1e3).toFixed(1)} KB`;
  return `${bytes} B`;
}

function formatUptime(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return h > 0 ? `${h}h ${m}m` : `${m}m`;
}

export default function Dashboard() {
  const { data: status, isLoading, error } = useNodeStatus();
  const { data: datasets } = useDatasets(true);
  const navigate = useNavigate();

  const handleGlobalSearch = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      const q = e.currentTarget.value.trim();
      if (q) navigate({ to: "/discover", search: { q } });
    }
  };

  if (error)
    return (
      <div className="p-6 text-agentprism-error">
        Failed to load node status:{" "}
        {error instanceof Error ? error.message : "Unknown error"}
      </div>
    );
  if (isLoading || !status)
    return (
      <div className="p-6 text-agentprism-muted-foreground">Loading…</div>
    );

  const totalSize =
    status.total_size_bytes ??
    (datasets ?? []).reduce((sum, d) => sum + (d.size_bytes ?? 0), 0);

  const cards = [
    { label: "Connected Peers", value: status.connected_peers ?? 0 },
    {
      label: "Published Datasets",
      value: status.published_datasets ?? (datasets ?? []).length,
    },
    { label: "Seeding", value: status.seeding_count },
    { label: "Total Size", value: formatSize(totalSize) },
    { label: "Uptime", value: formatUptime(status.uptime) },
  ];

  return (
    <div className="p-6 space-y-6">
      <div className="space-y-1">
        <h2 className="text-lg font-semibold">Node Dashboard</h2>
        <p className="text-xs text-agentprism-muted-foreground font-mono truncate">
          PeerID: {status.peer_id}
        </p>
        <p className="text-xs text-agentprism-muted-foreground font-mono truncate">
          DID: {status.did}
        </p>
      </div>

      {/* Global search → Discover */}
      <input
        type="text"
        onKeyDown={handleGlobalSearch}
        placeholder="Find datasets… (press Enter to discover)"
        className="w-full rounded-lg border border-agentprism-border bg-agentprism-card px-3 py-2 text-sm outline-none focus:border-agentprism-primary"
      />

      <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-5 gap-4">
        {cards.map((c) => (
          <div
            key={c.label}
            className="rounded-lg border border-agentprism-border bg-agentprism-card p-4"
          >
            <p className="text-xs text-agentprism-muted-foreground">
              {c.label}
            </p>
            <p className="text-2xl font-bold mt-1">{c.value}</p>
          </div>
        ))}
      </div>

      <div>
        <h3 className="text-sm font-semibold mb-2">Published Datasets</h3>
        {!datasets || datasets.length === 0 ? (
          <p className="text-sm text-agentprism-muted-foreground">
            No datasets published yet.
          </p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-agentprism-border text-left text-agentprism-muted-foreground">
                  <th className="py-2 pr-4">Title</th>
                  <th className="py-2 pr-4">CID</th>
                  <th className="py-2 pr-4 text-right">Size</th>
                </tr>
              </thead>
              <tbody>
                {datasets.map((d) => (
                  <tr
                    key={d.cid}
                    className="border-b border-agentprism-border/50"
                  >
                    <td className="py-2 pr-4">{d.title}</td>
                    <td className="py-2 pr-4 font-mono text-xs truncate max-w-[200px]">
                      {d.cid}
                    </td>
                    <td className="py-2 pr-4 text-right">
                      {formatSize(d.size_bytes ?? 0)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
}
