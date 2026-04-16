/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import { useEffect, useState } from "react";

interface NodeStatus {
  peer_id: string;
  did: string;
  uptime_secs: number;
  connected_peers: number;
  published_datasets: number;
  total_size_bytes: number;
  seeding_count: number;
}

interface SeedInfo {
  cid: string;
  title: string;
  size_bytes: number;
  info_hash: string;
}

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
  const [status, setStatus] = useState<NodeStatus | null>(null);
  const [seeds, setSeeds] = useState<SeedInfo[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([
      fetch("/api/node/status").then((r) =>
        r.ok ? r.json() : Promise.reject(new Error(`${r.status}`)),
      ),
      fetch("/api/datasets?mine=true").then((r) =>
        r.ok ? r.json() : Promise.resolve([]),
      ),
    ])
      .then(([s, d]) => {
        setStatus(s);
        setSeeds(d);
      })
      .catch((e) => setError(e.message));
  }, []);

  if (error)
    return (
      <div className="p-6 text-agentprism-error">
        Failed to load node status: {error}
      </div>
    );
  if (!status)
    return (
      <div className="p-6 text-agentprism-muted-foreground">Loading…</div>
    );

  const cards = [
    { label: "Connected Peers", value: status.connected_peers },
    { label: "Published Datasets", value: status.published_datasets },
    { label: "Seeding", value: status.seeding_count },
    { label: "Total Size", value: formatSize(status.total_size_bytes) },
    { label: "Uptime", value: formatUptime(status.uptime_secs) },
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
        {seeds.length === 0 ? (
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
                {seeds.map((s) => (
                  <tr
                    key={s.cid}
                    className="border-b border-agentprism-border/50"
                  >
                    <td className="py-2 pr-4">{s.title}</td>
                    <td className="py-2 pr-4 font-mono text-xs truncate max-w-[200px]">
                      {s.cid}
                    </td>
                    <td className="py-2 pr-4 text-right">
                      {formatSize(s.size_bytes)}
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
