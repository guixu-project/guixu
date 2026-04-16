/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import { useEffect, useState } from "react";

interface PeerInfo {
  peer_id: string;
  address: string;
  connected_since: string;
}

interface NatInfo {
  is_public: boolean;
  nat_type: string;
  relay_active: boolean;
  relay_address: string | null;
}

export default function Network() {
  const [peers, setPeers] = useState<PeerInfo[]>([]);
  const [nat, setNat] = useState<NatInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([
      fetch("/api/network/peers").then((r) =>
        r.ok ? r.json() : Promise.resolve([]),
      ),
      fetch("/api/network/nat").then((r) =>
        r.ok ? r.json() : Promise.resolve(null),
      ),
    ])
      .then(([p, n]) => {
        setPeers(p);
        setNat(n);
      })
      .catch((e) => setError(e.message));
  }, []);

  if (error)
    return (
      <div className="p-6 text-agentprism-error">
        Failed to load network info: {error}
      </div>
    );

  return (
    <div className="p-6 space-y-6">
      <h2 className="text-lg font-semibold">P2P Network</h2>

      {nat && (
        <div className="grid grid-cols-2 sm:grid-cols-4 gap-4">
          <div className="rounded-lg border border-agentprism-border bg-agentprism-card p-4">
            <p className="text-xs text-agentprism-muted-foreground">
              NAT Status
            </p>
            <p className="text-lg font-bold mt-1">
              {nat.is_public ? "Public" : "Behind NAT"}
            </p>
          </div>
          <div className="rounded-lg border border-agentprism-border bg-agentprism-card p-4">
            <p className="text-xs text-agentprism-muted-foreground">
              NAT Type
            </p>
            <p className="text-lg font-bold mt-1">{nat.nat_type}</p>
          </div>
          <div className="rounded-lg border border-agentprism-border bg-agentprism-card p-4">
            <p className="text-xs text-agentprism-muted-foreground">Relay</p>
            <p className="text-lg font-bold mt-1">
              {nat.relay_active ? "Active" : "Inactive"}
            </p>
          </div>
          <div className="rounded-lg border border-agentprism-border bg-agentprism-card p-4">
            <p className="text-xs text-agentprism-muted-foreground">
              Connected Peers
            </p>
            <p className="text-lg font-bold mt-1">{peers.length}</p>
          </div>
        </div>
      )}

      <div>
        <h3 className="text-sm font-semibold mb-2">Connected Peers</h3>
        {peers.length === 0 ? (
          <p className="text-sm text-agentprism-muted-foreground">
            No peers connected.
          </p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-agentprism-border text-left text-agentprism-muted-foreground">
                  <th className="py-2 pr-4">Peer ID</th>
                  <th className="py-2 pr-4">Address</th>
                </tr>
              </thead>
              <tbody>
                {peers.map((p) => (
                  <tr
                    key={p.peer_id}
                    className="border-b border-agentprism-border/50"
                  >
                    <td className="py-2 pr-4 font-mono text-xs truncate max-w-[300px]">
                      {p.peer_id}
                    </td>
                    <td className="py-2 pr-4 font-mono text-xs">
                      {p.address}
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
