# On-Chain Data Valuation for AI Agents: A Demo System Prototype

## 1. Problem Statement

AI agents increasingly need to autonomously discover, evaluate, and acquire datasets to complete tasks. However, no existing system provides:

1. **Task-aware data valuation**: A $100 dataset is worthless if irrelevant to the agent's task; a free dataset can have *negative* value if it degrades task performance (e.g., noisy labels, schema mismatch, outdated temporal coverage).
2. **On-chain usage feedback**: No transparent, tamper-proof record of how agents have used datasets and whether those datasets actually helped — analogous to product reviews on e-commerce platforms.
3. **Unified multi-source search**: Agents must manually query Kaggle, HuggingFace, IPFS, BitTorrent, and databases separately, with no cross-source ranking.
4. **Automated machine payments**: Paid datasets require human intervention for purchasing; no end-to-end agent-native payment flow exists.

## 2. System Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    AI Agent (MCP Client)                     │
│  "Find me datasets for predicting China GDP 2026, budget $5"│
└──────────────────────────┬──────────────────────────────────┘
                           │ MCP (JSON-RPC over stdio)
┌──────────────────────────▼──────────────────────────────────┐
│                     Guixu MCP Server                         │
│  Tools: dataset_search | dataset_evaluate | dataset_purchase │
│         dataset_verify  | dataset_feedback                   │
└──────┬──────────┬──────────┬──────────┬─────────────────────┘
       │          │          │          │
  ┌────▼───┐ ┌───▼────┐ ┌───▼────┐ ┌───▼─────┐
  │ Search │ │Valuatn │ │Trading │ │Feedback │
  │ Engine │ │ Engine │ │ Engine │ │ Engine  │
  └────┬───┘ └───┬────┘ └───┬────┘ └───┬─────┘
       │         │          │          │
  ┌────▼─────────▼──────────▼──────────▼──────┐
  │           Data Source Adapters              │
  │  Kaggle │ HuggingFace │ IPFS │ BT │ DB    │
  └────────────────────────────────────────────┘
  ┌───────────────────────────────────────────┐
  │         On-Chain Layer (Base L2)           │
  │  EAS Attestations: feedback + valuation    │
  │  x402 / MPP: automated payments            │
  └───────────────────────────────────────────┘
```

## 3. Core Design: Task-Aware Data Valuation

### 3.1 The Valuation Problem

Traditional data marketplaces assign a single price to a dataset. This is fundamentally wrong for AI agents because:

- **Context-dependent value**: The same weather dataset is highly valuable for a crop-yield prediction agent but useless for a sentiment analysis agent.
- **Negative value exists**: A dataset with systematic bias or outdated information can *harm* task performance. The agent should be warned away.
- **Marginal value matters**: If the agent already has 90% of the information, the marginal value of a new dataset is low regardless of its intrinsic quality.

### 3.2 Valuation Formula

We define the **Task-Conditioned Value (TCV)** of a dataset $D$ for task $T$ with agent context $C$:

```
TCV(D, T, C) = α · SchemaFit(D, T)
             + β · TemporalFit(D, T)
             + γ · InformationGain(D, C)
             + δ · QualityScore(D)
             + ε · CommunitySignal(D, T)
             - ζ · RiskPenalty(D)
```

Where:
- **SchemaFit** (weight α=0.25): Column-level semantic matching between dataset schema and task requirements. Uses embedding similarity between column names/descriptions and task description.
- **TemporalFit** (weight β=0.15): Overlap between dataset temporal coverage and task's required time range.
- **InformationGain** (weight γ=0.15): Marginal information the dataset adds beyond what the agent already has (measured via schema overlap with existing datasets).
- **QualityScore** (weight δ=0.10): Intrinsic data quality — completeness, consistency, freshness.
- **CommunitySignal** (weight ε=0.15): Aggregated on-chain feedback from previous agents who used this dataset for similar tasks.
- **RiskPenalty** (weight ζ=0.20): Negative signals — reports of data poisoning, schema drift, provider reputation issues. Weighted heavily so that negative community feedback can push TCV below zero.

TCV range: [-100, +100]. Negative values indicate the dataset would likely harm task performance.

### 3.3 On-Chain Feedback System

After an agent uses a dataset, it submits an **EAS (Ethereum Attestation Service) attestation** recording:

```json
{
  "schema": "0x...",  // EAS schema UID
  "data": {
    "dataset_cid": "bafybeig...",
    "task_type": "time_series_prediction",
    "task_description": "Predict China GDP 2026",
    "relevance_score": 0.92,      // -1.0 to 1.0
    "quality_rating": 4,           // 1-5 stars
    "task_success": true,
    "value_assessment": "positive", // positive | neutral | negative
    "agent_did": "did:key:z6Mk...",
    "timestamp": 1711234567
  }
}
```

These attestations are:
- **Immutable**: Once on-chain, cannot be altered
- **Attributable**: Linked to agent DID (Sybil-resistant via stake/gas cost)
- **Queryable**: Aggregated into CommunitySignal for future valuations
- **Task-typed**: Feedback is tagged with task type, so an agent doing "image classification" sees feedback from similar tasks, not unrelated ones

### 3.4 Valuation with Community Signal

The CommunitySignal component aggregates on-chain feedback:

```
CommunitySignal(D, T) = Σ_i w_i · relevance_i · task_similarity(T, T_i)
```

Where:
- `w_i` = reputation weight of feedback agent `i` (based on their own track record)
- `relevance_i` = the relevance score reported by agent `i`
- `task_similarity(T, T_i)` = cosine similarity between current task and the task for which feedback was given

This creates a **virtuous cycle**: agents that provide accurate feedback build reputation, their feedback carries more weight, which improves valuation accuracy for future agents.

## 4. Multi-Source Search Architecture

### 4.1 Unified Adapter Interface

Each data source implements a common `DataSourceAdapter` trait:

| Source | Discovery Method | Access Pattern | Payment |
|--------|-----------------|----------------|---------|
| Kaggle | REST API (`/datasets/list`) | HTTP download | Free / Competition |
| HuggingFace | Hub API (`/api/datasets`) | `datasets` library | Free / Gated |
| IPFS | CID resolution via gateway | HTTP gateway / P2P | Free |
| BitTorrent | DHT + info_hash | P2P swarm | Free / Paid (seller-only) |
| PostgreSQL | `information_schema` + SQL | SQL query | Internal |
| DuckDB | Catalog + SQL | SQL query | Internal |

### 4.2 Search Flow

```
Agent query: "China GDP data 2020-2025, time series format"
    │
    ├─ [Parallel] Kaggle adapter → 3 results
    ├─ [Parallel] HuggingFace adapter → 2 results
    ├─ [Parallel] IPFS adapter → 1 result
    ├─ [Parallel] P2P DHT → 4 results
    ├─ [Parallel] PostgreSQL catalog → 1 result
    │
    ├─ Merge + Deduplicate (by content hash)
    ├─ Compute TCV for each candidate
    ├─ Fetch on-chain feedback for each candidate
    ├─ Final ranking by TCV
    │
    └─ Return top-K results with valuation reports
```

## 5. Automated Payment Flow

For paid datasets, the system uses a two-protocol approach:

1. **x402 (Coinbase)**: For micropayments < $1 (previews, samples)
   - Agent sends HTTP request → receives 402 + payment details → signs USDC transfer → retries with payment proof

2. **Machine Payment Protocol (Stripe MPP)**: For session-based purchases
   - Agent opens payment session → multiple requests within budget → auto-settlement

Decision logic:
```
if amount < $0.01 → x402 single-shot
if session with same seller → MPP streaming
if amount > $1 and needs verification → ERC-8183 escrow
```

## 6. Demo Scenarios

### Scenario 1: Free Dataset Selection with Negative Value Detection

```
Agent: "I need image classification training data for medical X-rays"

Search returns:
  #1 "ChestX-ray14" (Kaggle, Q:88, Free, TCV: +82)
     → CommunitySignal: 47 agents used for medical imaging, avg relevance 0.91
  #2 "Random Images 2024" (HF, Q:45, Free, TCV: -15)
     → CommunitySignal: 3 agents reported "irrelevant noise, degraded model"
     → ⚠️ NEGATIVE VALUE: This dataset would likely harm your task
  #3 "MIMIC-CXR" (IPFS, Q:92, Free, TCV: +78)
     → CommunitySignal: 12 agents, avg relevance 0.87, but gated access

Agent automatically selects #1, avoids #2 despite being free.
```

### Scenario 2: Paid Dataset ROI with On-Chain History

```
Agent: "High-frequency trading data for emerging markets, budget $50"

Search returns:
  #1 "EM Tick Data" (P2P, Q:96, $50/mo, TCV: +71)
     → On-chain: 8 purchases, 7 positive reviews, avg ROI 3.2x
     → "Previous agents report 40% improvement in backtest accuracy"
  #2 "EM Daily OHLCV" (Kaggle, Q:75, Free, TCV: +45)
     → On-chain: 120 uses, mixed reviews for HFT tasks
     → "Free alternative but only daily granularity"

Agent: dataset_evaluate(#1) → ROI report shows marginal value over #2 justifies $50
Agent: dataset_purchase(#1) → x402 preview ($0.01) → ERC-8183 escrow ($50)
Agent: dataset_feedback(#1, relevance=0.93, success=true) → EAS attestation on-chain
```

## 7. Technical Architecture (Demo Scope)

For the demo prototype, we implement:

### In Scope (Core)
- Multi-source search with 6 adapters (Kaggle, HF, IPFS, BT, PostgreSQL, DuckDB)
- Task-Conditioned Valuation (TCV) engine with all 6 components
- On-chain feedback system via EAS attestations (simulated for demo)
- x402 payment flow (simulated)
- MCP server exposing 5 tools

### Simplified for Demo
- EAS attestations stored in local RocksDB (simulating on-chain)
- x402/MPP payments return mock receipts
- IPFS via HTTP gateway (no local node)
- BitTorrent via existing P2P layer
- Database adapters query local DuckDB/PostgreSQL instances

### Out of Scope
- ZKP attribute proofs
- Watermark embedding
- Real blockchain transactions
- Production-grade Sybil resistance
