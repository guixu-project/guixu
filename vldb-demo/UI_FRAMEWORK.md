<!--
Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
SPDX-License-Identifier: Apache-2.0
-->

# VLDB Demo UI Framework

## 1. Demo Goal

This UI is designed for a `VLDB demonstration paper and live demo`, not for a generic product landing page.

The screen should answer three questions in one glance:

1. `How does Guixu turn a user/agent request into a machine-executable task?`
2. `Why does Guixu choose one dataset over others, and what improvement does that choice produce?`
3. `How do decentralized assets, trades, and feedback feed future discovery and valuation?`

## 2. Recommended Layout

Use a single-screen cockpit layout.

- Header: `8%` height
- Top working area: `~62%` height
- Bottom ledger area: `~30%` height

Top working area split:

- Left: `Phase 1`, about `34%`
- Right: `Phase 2`, about `66%`

This keeps the eye on valuation and execution while preserving the workflow setup on the left.

## 3. Panel Responsibilities

### Phase 1: Agent Planning & Step-wise Discovery

Purpose:
- show the transformation from NL query to structured task profile
- show that dataset coarse search and code generation happen in parallel

Must show:
- natural-language query
- agent resource snapshot
- intent parsing output
- task description
- generated data keywords
- generated code summary or code spec

Should not show:
- full source code editor
- too many tiny workflow nodes
- long logs or parser internals

Recommended internal structure:

1. `Query Input`
   Example: "Train a helmet-detection model under a $10 budget."
2. `Resource Snapshot`
   CPU, memory, GPU, budget
3. `Intent Parsing`
   short explanation card showing extracted task type and constraints
4. `Task Profile`
   task description, metric target, annotation needs
5. `Discovery Preparation`
   keyword list and search cues
6. `Code Artifact`
   generated training template summary

Visual treatment:
- workflow card stack
- numbered nodes
- one clear split from parser output into:
  - keyword-based data coarse search
  - code generation

### Phase 2: Code-Aware Data Valuation & Execution

Purpose:
- this is the visual center of gravity
- explain why one dataset is selected and what happens after selection

Must show:
- candidate dataset list
- selected dataset
- valuation breakdown
- final recommendation
- execution status
- result improvement

Recommended internal structure:

1. `Candidate Datasets`
   compact ranked list, with source, price, license, and quick reputation hints
2. `Valuation Evidence`
   a large score and additive factors:
   - schema fit
   - label quality
   - domain relevance
   - diversity gain
   - expected training lift
   - cost
   - on-chain reputation
3. `Final Recommendation`
   recommended dataset + expected gain + cost
4. `Execution Result`
   training trace, progress, and final metric delta

Visual treatment:
- candidate list on the left
- evidence view in the center
- recommendation plus execution on the right

This area should visually dominate the page.

### Decentralized Asset Ledger & Feedback Loop

Purpose:
- demonstrate the decentralized marketplace and persistent asset memory
- make the reviewer understand that transactions and reviews help later discovery and valuation

Must show:
- asset source / seller identity
- trade or attestation history
- feedback / review memory

Recommended internal structure:

1. `Asset Source`
   seller DID, asset CID, price, attestation, source type
2. `Transaction Timeline`
   purchase and attestation events
3. `Feedback & Reputation`
   reviews, task-fit feedback, reputation summary

Important wording:

Add an explicit sentence in the UI similar to:

`Historical purchases, attestations, and reviews are reused as market memory for future ranking and valuation.`

Without this, the on-chain zone risks looking bolted on.

## 4. Visual Direction

Target vibe:
- academic instrument panel
- clean systems demo
- slightly premium, but not startup-marketing glossy

Recommended palette:

- Background: warm ivory / paper tone
- Primary frame: slate blue
- Accent: teal
- Supporting highlight: muted amber
- Risk/error: brick red

Typography direction:
- strong grotesk or technical sans for titles
- smaller compact sans for data labels

Avoid:
- dark cyberpunk blockchain aesthetic
- dashboard overload
- overly playful product illustrations

## 5. Content Priorities

### Highest Priority

- recommendation rationale
- valuation factors
- execution gain

### Medium Priority

- task parsing summary
- code artifact summary
- candidate comparison

### Lower Priority

- generic marketplace browsing
- chain/network mode toggles
- decorative blockchain motifs

## 6. Demo Paper Screenshot Strategy

The paper screenshot should capture a `stable completed state`, not a blank or partially initialized state.

The ideal screenshot already contains:

- input query
- parsed task profile
- candidate list
- recommended dataset
- valuation evidence
- execution outcome
- at least one purchase event
- at least one feedback event

That allows one figure to communicate the full system loop.

## 7. Frontend Implementation Notes

When this turns into a real UI, keep the panel model explicit:

- `query panel view model`
- `planning panel view model`
- `candidate list view model`
- `valuation result view model`
- `execution result view model`
- `ledger/reputation view model`

Do not bind the frontend directly to raw MCP strings. The UI should consume typed objects with consistent numeric fields.

## 8. Initial Demo Scenario

Current canonical scenario in the mockup:

- Task: helmet detection model training
- Budget: `$10`
- Constraints: bounding boxes, construction-domain images, acceptable annotation quality
- Desired result: improved `mAP`

This scenario is strong because it naturally ties together:

- intent parsing
- code generation
- data discovery
- code-aware valuation
- execution result
- decentralized asset pricing and review memory

