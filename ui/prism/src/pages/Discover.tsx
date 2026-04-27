/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import { useState, useCallback } from "react";
import { useIntentParse, useMcpSearch, useMcpEvaluate } from "../api";
import type { QueryProfile, MarketDataset } from "../api/types";

// --- Helpers ---

function formatSize(bytes: number): string {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  if (bytes >= 1e6) return `${(bytes / 1e6).toFixed(1)} MB`;
  if (bytes >= 1e3) return `${(bytes / 1e3).toFixed(1)} KB`;
  return `${bytes} B`;
}

// --- Intent Panel (left) ---

function IntentPanel({
  profile,
  onProfileChange,
  onSearch,
  isParsing,
  isSearching,
}: {
  profile: QueryProfile;
  onProfileChange: (p: QueryProfile) => void;
  onSearch: () => void;
  isParsing: boolean;
  isSearching: boolean;
}) {
  const set = (k: keyof QueryProfile, v: string | string[]) =>
    onProfileChange({ ...profile, [k]: v });

  return (
    <div className="flex flex-col gap-3 p-4 h-full overflow-y-auto">
      <h3 className="text-xs font-semibold text-agentprism-muted-foreground uppercase tracking-wide">
        Query Profile
      </h3>

      <label className="text-xs text-agentprism-muted-foreground">
        Task Type
        <select
          value={profile.task_type}
          onChange={(e) => set("task_type", e.target.value)}
          className="mt-1 block w-full rounded border border-agentprism-border bg-agentprism-card px-2 py-1.5 text-sm"
        >
          {["classification", "detection", "segmentation", "forecasting", "ranking", "retrieval", "generation", "summarization", "evaluation"].map(
            (t) => (
              <option key={t} value={t}>
                {t}
              </option>
            ),
          )}
        </select>
      </label>

      <label className="text-xs text-agentprism-muted-foreground">
        Keywords
        <input
          value={profile.keywords.join(", ")}
          onChange={(e) =>
            set(
              "keywords",
              e.target.value.split(",").map((s) => s.trim()).filter(Boolean),
            )
          }
          className="mt-1 block w-full rounded border border-agentprism-border bg-agentprism-card px-2 py-1.5 text-sm"
          placeholder="cat, image, pet"
        />
      </label>

      <label className="text-xs text-agentprism-muted-foreground">
        Modality
        <select
          value={profile.sample_unit}
          onChange={(e) => set("sample_unit", e.target.value)}
          className="mt-1 block w-full rounded border border-agentprism-border bg-agentprism-card px-2 py-1.5 text-sm"
        >
          {["", "image", "video", "text", "tabular", "audio"].map((m) => (
            <option key={m} value={m}>
              {m || "any"}
            </option>
          ))}
        </select>
      </label>

      <label className="text-xs text-agentprism-muted-foreground">
        Target Entity
        <input
          value={profile.target_entity ?? ""}
          onChange={(e) => set("target_entity", e.target.value)}
          className="mt-1 block w-full rounded border border-agentprism-border bg-agentprism-card px-2 py-1.5 text-sm"
          placeholder="e.g. cat, lung nodule"
        />
      </label>

      <label className="text-xs text-agentprism-muted-foreground">
        Budget
        <input
          value={profile.budget ?? ""}
          onChange={(e) => set("budget", e.target.value)}
          className="mt-1 block w-full rounded border border-agentprism-border bg-agentprism-card px-2 py-1.5 text-sm"
          placeholder="0 USD"
        />
      </label>

      <button
        onClick={onSearch}
        disabled={isParsing || isSearching}
        className="mt-2 rounded bg-agentprism-primary text-agentprism-primary-foreground px-3 py-2 text-sm font-medium disabled:opacity-50"
      >
        {isParsing ? "Parsing…" : isSearching ? "Searching…" : "Search Datasets"}
      </button>
    </div>
  );
}

// --- Results Table (center) ---

function ResultsTable({
  results,
  selected,
  compared,
  onSelect,
  onToggleCompare,
  onEvaluate,
  isEvaluating,
}: {
  results: MarketDataset[];
  selected: string | null;
  compared: Set<string>;
  onSelect: (cid: string) => void;
  onToggleCompare: (cid: string) => void;
  onEvaluate: (cid: string) => void;
  isEvaluating: boolean;
}) {
  if (results.length === 0) return null;

  return (
    <div className="overflow-auto h-full">
      <table className="w-full text-xs">
        <thead className="sticky top-0 bg-agentprism-background">
          <tr className="border-b border-agentprism-border text-left text-agentprism-muted-foreground">
            <th className="py-2 px-2">Title</th>
            <th className="py-2 px-2">Cost</th>
            <th className="py-2 px-2">Access</th>
            <th className="py-2 px-2 text-right">Rows</th>
            <th className="py-2 px-2 text-right">Size</th>
            <th className="py-2 px-2">Actions</th>
          </tr>
        </thead>
        <tbody>
          {results.map((r) => (
            <tr
              key={r.cid}
              onClick={() => onSelect(r.cid)}
              className={`border-b border-agentprism-border/50 cursor-pointer hover:bg-agentprism-card/50 ${
                selected === r.cid ? "bg-agentprism-card" : ""
              }`}
            >
              <td className="py-2 px-2 max-w-[200px] truncate">{r.title}</td>
              <td className="py-2 px-2">
                <span
                  className={`rounded px-1.5 py-0.5 text-[10px] font-medium ${
                    r.price.amount > 0
                      ? "bg-amber-500/10 text-amber-400"
                      : "bg-emerald-500/10 text-emerald-400"
                  }`}
                >
                  {r.price.amount > 0 ? `$${r.price.amount.toFixed(2)}` : "Free"}
                </span>
              </td>
              <td className="py-2 px-2 text-agentprism-muted-foreground">
                {r.access}
              </td>
              <td className="py-2 px-2 text-right tabular-nums">
                {r.row_count.toLocaleString()}
              </td>
              <td className="py-2 px-2 text-right tabular-nums">
                {formatSize(r.size_bytes)}
              </td>
              <td className="py-2 px-2">
                <div className="flex gap-1" onClick={(e) => e.stopPropagation()}>
                  <button
                    onClick={() => onToggleCompare(r.cid)}
                    className={`rounded px-1.5 py-0.5 text-[10px] ${
                      compared.has(r.cid)
                        ? "bg-agentprism-primary text-agentprism-primary-foreground"
                        : "bg-agentprism-muted text-agentprism-muted-foreground hover:bg-agentprism-card"
                    }`}
                  >
                    {compared.has(r.cid) ? "✓ Cmp" : "Cmp"}
                  </button>
                  <button
                    onClick={() => onEvaluate(r.cid)}
                    disabled={isEvaluating}
                    className="rounded bg-agentprism-muted px-1.5 py-0.5 text-[10px] text-agentprism-muted-foreground hover:bg-agentprism-card disabled:opacity-50"
                  >
                    Eval
                  </button>
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

// --- Compare Tray ---

function CompareTray({
  items,
  evaluations,
  onRemove,
}: {
  items: MarketDataset[];
  evaluations: Map<string, Record<string, unknown>>;
  onRemove: (cid: string) => void;
}) {
  if (items.length === 0) return null;

  return (
    <div className="border-t border-agentprism-border p-3">
      <h4 className="text-xs font-semibold text-agentprism-muted-foreground mb-2">
        Compare ({items.length})
      </h4>
      <div className="flex gap-2 overflow-x-auto">
        {items.map((d) => {
          const ev = evaluations.get(d.cid);
          const score = ev ? String(ev.tcv_score ?? ev.score ?? "") : "";
          return (
            <div
              key={d.cid}
              className="shrink-0 rounded border border-agentprism-border bg-agentprism-card p-2 w-[160px]"
            >
              <p className="text-xs font-medium truncate">{d.title}</p>
              {score && (
                <p className="text-lg font-bold mt-1">TCV {score}</p>
              )}
              <p className="text-[10px] text-agentprism-muted-foreground">
                {d.price.amount > 0 ? `$${d.price.amount.toFixed(2)}` : "Free"}
              </p>
              <button
                onClick={() => onRemove(d.cid)}
                className="text-[10px] text-agentprism-error mt-1"
              >
                Remove
              </button>
            </div>
          );
        })}
      </div>
    </div>
  );
}

// --- Dataset Inspector (right) ---

function DatasetInspector({
  dataset,
  evaluation,
}: {
  dataset: MarketDataset | null;
  evaluation: Record<string, unknown> | undefined;
}) {
  if (!dataset)
    return (
      <div className="flex items-center justify-center h-full text-agentprism-muted-foreground text-xs">
        Select a dataset to inspect
      </div>
    );

  return (
    <div className="p-4 h-full overflow-y-auto space-y-4">
      <div>
        <h3 className="font-medium text-sm">{dataset.title}</h3>
        {dataset.description && (
          <p className="text-xs text-agentprism-muted-foreground mt-1">
            {dataset.description}
          </p>
        )}
      </div>

      <div className="grid grid-cols-2 gap-2 text-xs">
        <div>
          <span className="text-agentprism-muted-foreground">CID</span>
          <p className="font-mono truncate">{dataset.cid}</p>
        </div>
        <div>
          <span className="text-agentprism-muted-foreground">Provider</span>
          <p>{dataset.provider}</p>
        </div>
        <div>
          <span className="text-agentprism-muted-foreground">Access</span>
          <p>{dataset.access}</p>
        </div>
        <div>
          <span className="text-agentprism-muted-foreground">Price</span>
          <p>
            {dataset.price.amount > 0
              ? `$${dataset.price.amount.toFixed(2)}`
              : "Free"}
          </p>
        </div>
        <div>
          <span className="text-agentprism-muted-foreground">Rows</span>
          <p>{dataset.row_count.toLocaleString()}</p>
        </div>
        <div>
          <span className="text-agentprism-muted-foreground">Size</span>
          <p>{formatSize(dataset.size_bytes)}</p>
        </div>
      </div>

      {dataset.tags.length > 0 && (
        <div className="flex flex-wrap gap-1">
          {dataset.tags.map((t) => (
            <span
              key={t}
              className="rounded bg-agentprism-muted px-1.5 py-0.5 text-[10px] text-agentprism-muted-foreground"
            >
              {t}
            </span>
          ))}
        </div>
      )}

      {evaluation && (
        <div className="rounded border border-agentprism-border bg-agentprism-card p-3 space-y-2">
          <h4 className="text-xs font-semibold">TCV Evaluation</h4>
          {"tcv_score" in evaluation && (
            <p className="text-2xl font-bold">
              {String(evaluation.tcv_score)}
            </p>
          )}
          {"score" in evaluation && !("tcv_score" in evaluation) && (
            <p className="text-2xl font-bold">
              {String(evaluation.score)}
            </p>
          )}
          {"explanation" in evaluation && (
            <p className="text-xs text-agentprism-muted-foreground">
              {String(evaluation.explanation)}
            </p>
          )}
          {Array.isArray(evaluation.risk_flags) &&
            (evaluation.risk_flags as string[]).length > 0 && (
              <div className="flex flex-wrap gap-1">
                {(evaluation.risk_flags as string[]).map((f, i) => (
                  <span
                    key={i}
                    className="rounded bg-red-500/10 text-red-400 px-1.5 py-0.5 text-[10px]"
                  >
                    {f}
                  </span>
                ))}
              </div>
            )}
        </div>
      )}

      {/* Schema / Preview / Reviews placeholders */}
      <div className="rounded border border-agentprism-border/50 p-3">
        <p className="text-[10px] text-agentprism-muted-foreground">
          Schema preview — coming in Phase 2
        </p>
      </div>
    </div>
  );
}

// --- Main Discover Page ---

const EMPTY_PROFILE: QueryProfile = {
  query: "",
  task_type: "classification",
  keywords: [],
  sample_unit: "",
};

export default function Discover({ initialQuery }: { initialQuery?: string }) {
  const [taskInput, setTaskInput] = useState(initialQuery ?? "");
  const [profile, setProfile] = useState<QueryProfile>({
    ...EMPTY_PROFILE,
    query: initialQuery ?? "",
  });
  const [results, setResults] = useState<MarketDataset[]>([]);
  const [selectedCid, setSelectedCid] = useState<string | null>(null);
  const [compared, setCompared] = useState<Set<string>>(new Set());
  const [evaluations, setEvaluations] = useState<Map<string, Record<string, unknown>>>(new Map());

  const intentParse = useIntentParse();
  const mcpSearch = useMcpSearch();
  const mcpEvaluate = useMcpEvaluate();

  const handleSearch = useCallback(async () => {
    if (!taskInput.trim()) return;

    // Step 1: intent_parse
    const parsed = await intentParse.mutateAsync({
      ...profile,
      query: taskInput,
    });
    const merged: QueryProfile = {
      ...profile,
      ...parsed,
      query: taskInput,
    };
    setProfile(merged);

    // Step 2: dataset_search
    const searchResult = await mcpSearch.mutateAsync({
      query: merged.keywords.join(" ") || taskInput,
      task_type: merged.task_type,
    });
    setResults(searchResult?.results ?? []);
    setSelectedCid(null);
  }, [taskInput, profile, intentParse, mcpSearch]);

  const handleEvaluate = useCallback(
    async (cid: string) => {
      const result = await mcpEvaluate.mutateAsync({
        cid,
        task_description: profile.task_description ?? taskInput,
        task_type: profile.task_type,
      });
      setEvaluations((prev) => new Map(prev).set(cid, result));
    },
    [profile, taskInput, mcpEvaluate],
  );

  const toggleCompare = (cid: string) => {
    setCompared((prev) => {
      const next = new Set(prev);
      if (next.has(cid)) next.delete(cid);
      else if (next.size < 5) next.add(cid);
      return next;
    });
  };

  const selectedDataset = results.find((r) => r.cid === selectedCid) ?? null;
  const comparedItems = results.filter((r) => compared.has(r.cid));

  return (
    <div className="flex flex-col h-full">
      {/* Task input bar */}
      <div className="flex gap-2 p-4 border-b border-agentprism-border shrink-0">
        <input
          type="text"
          value={taskInput}
          onChange={(e) => setTaskInput(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleSearch()}
          placeholder="Describe your data need, e.g. 'Find image datasets for cat breed classification'…"
          className="flex-1 rounded-lg border border-agentprism-border bg-agentprism-card px-3 py-2 text-sm outline-none focus:border-agentprism-primary"
        />
        <button
          onClick={handleSearch}
          disabled={intentParse.isPending || mcpSearch.isPending}
          className="rounded-lg bg-agentprism-primary text-agentprism-primary-foreground px-4 py-2 text-sm font-medium disabled:opacity-50"
        >
          {intentParse.isPending
            ? "Parsing…"
            : mcpSearch.isPending
              ? "Searching…"
              : "Discover"}
        </button>
      </div>

      {/* Error display */}
      {(intentParse.error || mcpSearch.error) && (
        <div className="px-4 py-2 text-xs text-agentprism-error">
          {String(intentParse.error?.message ?? mcpSearch.error?.message)}
        </div>
      )}

      {/* Three-panel layout */}
      <div className="flex flex-1 min-h-0">
        {/* Left: Intent Panel */}
        <div className="w-[220px] shrink-0 border-r border-agentprism-border">
          <IntentPanel
            profile={profile}
            onProfileChange={setProfile}
            onSearch={handleSearch}
            isParsing={intentParse.isPending}
            isSearching={mcpSearch.isPending}
          />
        </div>

        {/* Center: Results + Compare */}
        <div className="flex-1 flex flex-col min-w-0">
          {results.length === 0 && !mcpSearch.isPending ? (
            <div className="flex items-center justify-center h-full text-agentprism-muted-foreground text-sm">
              Enter a task above to discover datasets
            </div>
          ) : (
            <ResultsTable
              results={results}
              selected={selectedCid}
              compared={compared}
              onSelect={setSelectedCid}
              onToggleCompare={toggleCompare}
              onEvaluate={handleEvaluate}
              isEvaluating={mcpEvaluate.isPending}
            />
          )}
          <CompareTray
            items={comparedItems}
            evaluations={evaluations}
            onRemove={(cid) => toggleCompare(cid)}
          />
        </div>

        {/* Right: Dataset Inspector */}
        <div className="w-[300px] shrink-0 border-l border-agentprism-border">
          <DatasetInspector
            dataset={selectedDataset}
            evaluation={selectedCid ? evaluations.get(selectedCid) : undefined}
          />
        </div>
      </div>
    </div>
  );
}
