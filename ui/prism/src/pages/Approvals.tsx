/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import { useApprovals, useApproveTask } from "../api";

const RISK_COLORS: Record<string, string> = {
  low: "bg-emerald-500/10 text-emerald-400",
  medium: "bg-amber-500/10 text-amber-400",
  high: "bg-red-500/10 text-red-400",
};

export default function Approvals() {
  const { data: tasks, isLoading, error } = useApprovals();
  const approve = useApproveTask();

  if (error) return <div className="p-6 text-agentprism-error">Failed to load approvals</div>;
  if (isLoading) return <div className="p-6 text-agentprism-muted-foreground">Loading…</div>;

  const pending = (tasks ?? []).filter((t) => t.status === "pending");
  const resolved = (tasks ?? []).filter((t) => t.status !== "pending");

  return (
    <div className="p-6 space-y-6">
      <h2 className="text-lg font-semibold">Approval Center</h2>

      {/* Pending */}
      <div>
        <h3 className="text-sm font-semibold mb-2">Pending ({pending.length})</h3>
        {pending.length === 0 ? (
          <p className="text-sm text-agentprism-muted-foreground">No pending approvals.</p>
        ) : (
          <div className="space-y-3">
            {pending.map((t) => (
              <div key={t.id} className="rounded-lg border border-agentprism-border bg-agentprism-card p-4 space-y-2">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <span className="font-mono text-sm">{t.tool_name}</span>
                    <span className={`rounded px-1.5 py-0.5 text-[10px] ${RISK_COLORS[t.risk_level] ?? ""}`}>
                      {t.risk_level}
                    </span>
                  </div>
                  <span className="text-[10px] text-agentprism-muted-foreground">{new Date(t.created_at).toLocaleString()}</span>
                </div>
                <pre className="text-xs bg-agentprism-background rounded p-2 overflow-x-auto max-h-24">
                  {JSON.stringify(t.arguments, null, 2)}
                </pre>
                {t.session_id && <p className="text-[10px] text-agentprism-muted-foreground">Session: {t.session_id}</p>}
                <div className="flex gap-2">
                  <button
                    onClick={() => approve.mutate({ task_id: t.id, decision: "approve" })}
                    disabled={approve.isPending}
                    className="rounded bg-emerald-600 text-white px-3 py-1 text-xs disabled:opacity-50"
                  >
                    Approve
                  </button>
                  <button
                    onClick={() => approve.mutate({ task_id: t.id, decision: "reject" })}
                    disabled={approve.isPending}
                    className="rounded bg-red-600 text-white px-3 py-1 text-xs disabled:opacity-50"
                  >
                    Deny
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Resolved */}
      {resolved.length > 0 && (
        <div>
          <h3 className="text-sm font-semibold mb-2">Resolved ({resolved.length})</h3>
          <div className="space-y-2">
            {resolved.map((t) => (
              <div key={t.id} className="rounded border border-agentprism-border/50 p-3 flex items-center justify-between text-xs">
                <span className="font-mono">{t.tool_name}</span>
                <span className={`rounded px-1.5 py-0.5 text-[10px] ${t.status === "approved" ? "bg-emerald-500/10 text-emerald-400" : "bg-red-500/10 text-red-400"}`}>
                  {t.status}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
