/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import { useState, useRef } from "react";
import { usePublish } from "../api";

type Step = "file" | "describe" | "privacy" | "price" | "review";
const STEPS: { key: Step; label: string }[] = [
  { key: "file", label: "Select File" },
  { key: "describe", label: "Describe" },
  { key: "privacy", label: "Privacy" },
  { key: "price", label: "Price" },
  { key: "review", label: "Review & Publish" },
];

export default function Publish() {
  const [step, setStep] = useState<Step>("file");
  const [file, setFile] = useState<File | null>(null);
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [tags, setTags] = useState("");
  const [privacy, setPrivacy] = useState<"off" | "standard" | "strict">("off");
  const [epsilon, setEpsilon] = useState("1.0");
  const [access, setAccess] = useState<"open" | "paid">("open");
  const [price, setPrice] = useState("0");
  const fileRef = useRef<HTMLInputElement>(null);
  const publish = usePublish();

  const stepIdx = STEPS.findIndex((s) => s.key === step);
  const next = () => setStep(STEPS[Math.min(stepIdx + 1, STEPS.length - 1)].key);
  const prev = () => setStep(STEPS[Math.max(stepIdx - 1, 0)].key);

  const handlePublish = async () => {
    if (!file) return;
    const form = new FormData();
    form.append("file", file);
    form.append("title", title || file.name);
    form.append("description", description);
    form.append("tags", tags);
    form.append("access", access);
    form.append("price", access === "paid" ? price : "0");
    form.append("privacy_level", privacy);
    form.append("epsilon", epsilon);
    await publish.mutateAsync(form);
  };

  return (
    <div className="p-6 max-w-2xl mx-auto space-y-6">
      <h2 className="text-lg font-semibold">Publish Dataset</h2>

      {/* Step indicator */}
      <div className="flex gap-1 text-xs">
        {STEPS.map((s, i) => (
          <button
            key={s.key}
            onClick={() => setStep(s.key)}
            className={`px-2 py-1 rounded ${i === stepIdx ? "bg-agentprism-primary text-agentprism-primary-foreground" : "text-agentprism-muted-foreground"}`}
          >
            {i + 1}. {s.label}
          </button>
        ))}
      </div>

      {/* Step content */}
      <div className="rounded-lg border border-agentprism-border bg-agentprism-card p-6 space-y-4">
        {step === "file" && (
          <>
            <p className="text-sm">Select a CSV, Parquet, JSON, or TSV file to publish.</p>
            <input ref={fileRef} type="file" accept=".csv,.parquet,.json,.tsv,.jsonl" onChange={(e) => setFile(e.target.files?.[0] ?? null)} className="text-sm" />
            {file && <p className="text-xs text-agentprism-muted-foreground">{file.name} — {(file.size / 1024).toFixed(1)} KB</p>}
          </>
        )}

        {step === "describe" && (
          <>
            <label className="block text-xs text-agentprism-muted-foreground">
              Title
              <input value={title} onChange={(e) => setTitle(e.target.value)} placeholder={file?.name ?? "Dataset title"} className="mt-1 block w-full rounded border border-agentprism-border bg-agentprism-background px-2 py-1.5 text-sm" />
            </label>
            <label className="block text-xs text-agentprism-muted-foreground">
              Description
              <textarea value={description} onChange={(e) => setDescription(e.target.value)} rows={3} className="mt-1 block w-full rounded border border-agentprism-border bg-agentprism-background px-2 py-1.5 text-sm" />
            </label>
            <label className="block text-xs text-agentprism-muted-foreground">
              Tags (comma-separated)
              <input value={tags} onChange={(e) => setTags(e.target.value)} placeholder="finance, timeseries" className="mt-1 block w-full rounded border border-agentprism-border bg-agentprism-background px-2 py-1.5 text-sm" />
            </label>
          </>
        )}

        {step === "privacy" && (
          <>
            <p className="text-sm">Privacy processing level</p>
            {(["off", "standard", "strict"] as const).map((p) => (
              <label key={p} className="flex items-center gap-2 text-sm">
                <input type="radio" name="privacy" checked={privacy === p} onChange={() => setPrivacy(p)} />
                <span className="capitalize">{p}</span>
              </label>
            ))}
            {privacy !== "off" && (
              <label className="block text-xs text-agentprism-muted-foreground mt-2">
                Epsilon (ε)
                <input type="number" step="0.1" value={epsilon} onChange={(e) => setEpsilon(e.target.value)} className="mt-1 block w-32 rounded border border-agentprism-border bg-agentprism-background px-2 py-1.5 text-sm" />
              </label>
            )}
          </>
        )}

        {step === "price" && (
          <>
            <p className="text-sm">Access & pricing</p>
            {(["open", "paid"] as const).map((a) => (
              <label key={a} className="flex items-center gap-2 text-sm">
                <input type="radio" name="access" checked={access === a} onChange={() => setAccess(a)} />
                <span className="capitalize">{a}</span>
              </label>
            ))}
            {access === "paid" && (
              <label className="block text-xs text-agentprism-muted-foreground mt-2">
                Price (USDC)
                <input type="number" step="0.01" value={price} onChange={(e) => setPrice(e.target.value)} className="mt-1 block w-32 rounded border border-agentprism-border bg-agentprism-background px-2 py-1.5 text-sm" />
              </label>
            )}
          </>
        )}

        {step === "review" && (
          <>
            <h3 className="text-sm font-semibold">Review</h3>
            <dl className="grid grid-cols-2 gap-2 text-xs">
              <dt className="text-agentprism-muted-foreground">File</dt><dd>{file?.name ?? "—"}</dd>
              <dt className="text-agentprism-muted-foreground">Title</dt><dd>{title || file?.name || "—"}</dd>
              <dt className="text-agentprism-muted-foreground">Privacy</dt><dd>{privacy}</dd>
              <dt className="text-agentprism-muted-foreground">Access</dt><dd>{access}</dd>
              <dt className="text-agentprism-muted-foreground">Price</dt><dd>{access === "paid" ? `$${price}` : "Free"}</dd>
            </dl>
            {publish.isSuccess && <p className="text-sm text-emerald-400">✓ Published successfully</p>}
            {publish.error && <p className="text-sm text-agentprism-error">{publish.error instanceof Error ? publish.error.message : "Publish failed"}</p>}
          </>
        )}
      </div>

      {/* Navigation */}
      <div className="flex justify-between">
        <button onClick={prev} disabled={stepIdx === 0} className="rounded px-4 py-2 text-sm bg-agentprism-muted text-agentprism-muted-foreground disabled:opacity-30">Back</button>
        {step === "review" ? (
          <button onClick={handlePublish} disabled={!file || publish.isPending} className="rounded px-4 py-2 text-sm bg-agentprism-primary text-agentprism-primary-foreground disabled:opacity-50">
            {publish.isPending ? "Publishing…" : "Publish"}
          </button>
        ) : (
          <button onClick={next} disabled={step === "file" && !file} className="rounded px-4 py-2 text-sm bg-agentprism-primary text-agentprism-primary-foreground disabled:opacity-50">Next</button>
        )}
      </div>
    </div>
  );
}
