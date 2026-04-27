/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import { useNetworkPeers, useNetworkNat } from "../api";

export default function Network() {
  const {
    data: peersData,
    error: peersError,
  } = useNetworkPeers();
  const { data: nat } = useNetworkNat();

  const peers = peersData?.peers ?? [];

  if (peersError)
    return (
      <div className="p-6 text-agentprism-error">
        Failed to load network info:{" "}
        {peersError instanceof Error ? peersError.message : "Unknown error"}
      </div>
    );

  return (
    <div className="p-6 space-y-6">
      <h2 className="text-lg font-semibold">P2P Network</h2>

      {nat && (
        <div className="grid grid-cols-2 sm:grid-cols-4 gap-4">
          <div className="rounded-lg border border-agentprism-border bg-agentprism-card p-4">
            <p className="text-xs text-agentprism-muted-foreground">
              NAT Type
            </p>
            <p className="text-lg font-bold mt-1">{nat.nat_type}</p>
          </div>
          <div className="rounded-lg border border-agentprism-border bg-agentprism-card p-4">
            <p className="text-xs text-agentprism-muted-foreground">Relay</p>
            <p className="text-lg font-bold mt-1">
              {nat.relay_enabled ? "Enabled" : "Disabled"}
            </p>
          </div>
          <div className="rounded-lg border border-agentprism-border bg-agentprism-card p-4">
            <p className="text-xs text-agentprism-muted-foreground">
              Connected Peers
            </p>
            <p className="text-lg font-bold mt-1">{peers.length}</p>
          </div>
          {peersData?.local_peer_id && (
            <div className="rounded-lg border border-agentprism-border bg-agentprism-card p-4">
              <p className="text-xs text-agentprism-muted-foreground">
                Local Peer
              </p>
              <p className="text-xs font-mono mt-1 truncate">
                {peersData.local_peer_id}
              </p>
            </div>
          )}
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
