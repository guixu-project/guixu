/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import { useWalletBalance, useWalletTransactions } from "../api";

export default function Wallet() {
  const { data: balance, isLoading: balLoading } = useWalletBalance();
  const { data: txns, isLoading: txLoading } = useWalletTransactions();

  return (
    <div className="p-6 space-y-6">
      <h2 className="text-lg font-semibold">Wallet</h2>

      {/* Balance card */}
      <div className="rounded-lg border border-agentprism-border bg-agentprism-card p-6">
        <p className="text-xs text-agentprism-muted-foreground">Balance</p>
        {balLoading ? (
          <p className="text-2xl font-bold mt-1 text-agentprism-muted-foreground">…</p>
        ) : (
          <p className="text-3xl font-bold mt-1">
            {balance?.balance?.toFixed(2) ?? "0.00"}{" "}
            <span className="text-sm text-agentprism-muted-foreground">{balance?.currency ?? "USDC"}</span>
          </p>
        )}
      </div>

      {/* Transactions */}
      <div>
        <h3 className="text-sm font-semibold mb-2">Transactions</h3>
        {txLoading ? (
          <p className="text-sm text-agentprism-muted-foreground">Loading…</p>
        ) : !txns || txns.length === 0 ? (
          <p className="text-sm text-agentprism-muted-foreground">No transactions yet.</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-xs">
              <thead>
                <tr className="border-b border-agentprism-border text-left text-agentprism-muted-foreground">
                  <th className="py-2 pr-3">Type</th>
                  <th className="py-2 pr-3 text-right">Amount</th>
                  <th className="py-2 pr-3">Status</th>
                  <th className="py-2 pr-3">Dataset</th>
                  <th className="py-2 pr-3">Time</th>
                </tr>
              </thead>
              <tbody>
                {txns.map((tx) => (
                  <tr key={tx.id} className="border-b border-agentprism-border/50">
                    <td className="py-2 pr-3 capitalize">{tx.type}</td>
                    <td className="py-2 pr-3 text-right tabular-nums">
                      {tx.amount.toFixed(2)} {tx.currency}
                    </td>
                    <td className="py-2 pr-3">
                      <span className={`rounded px-1.5 py-0.5 text-[10px] ${tx.status === "completed" ? "bg-emerald-500/10 text-emerald-400" : tx.status === "failed" ? "bg-red-500/10 text-red-400" : "bg-amber-500/10 text-amber-400"}`}>
                        {tx.status}
                      </span>
                    </td>
                    <td className="py-2 pr-3 font-mono truncate max-w-[120px]">{tx.dataset_cid ?? "—"}</td>
                    <td className="py-2 pr-3 text-agentprism-muted-foreground">{new Date(tx.timestamp).toLocaleString()}</td>
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
