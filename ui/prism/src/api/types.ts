/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

// --- Node ---

export interface NodeStatus {
  status: string;
  did: string;
  peer_id: string;
  seeding_count: number;
  uptime: number;
  version: string;
  connected_peers?: number;
  published_datasets?: number;
  total_size_bytes?: number;
}

// --- Datasets ---

export interface DatasetMeta {
  cid: string;
  title: string;
  description?: string;
  tags?: string[];
  row_count?: number;
  size_bytes?: number;
  schema?: { columns: SchemaColumn[] };
  price?: { amount: number; currency: string };
  provider?: string;
  access?: string;
  info_hash?: string;
}

export interface SchemaColumn {
  name: string;
  dtype: string;
}

// --- Network ---

export interface PeersResponse {
  local_peer_id: string;
  peers: PeerInfo[];
}

export interface PeerInfo {
  peer_id: string;
  address: string;
  connected_since?: string;
}

export interface NatInfo {
  nat_type: string;
  relay_enabled: boolean;
}

// --- Market ---

export interface MarketSearchResponse {
  results: MarketDataset[];
}

export interface MarketDataset {
  cid: string;
  title: string;
  description: string | null;
  tags: string[];
  row_count: number;
  size_bytes: number;
  price: { amount: number; currency: string };
  provider: string;
  access: string;
}

export interface MarketPreview {
  cid: string;
  schema?: { columns: SchemaColumn[] };
  source?: string;
}

// --- MCP Tool Call ---

export interface McpRequest {
  jsonrpc: "2.0";
  id: number;
  method: "tools/call";
  params: { name: string; arguments: Record<string, unknown> };
}

export interface McpTextContent {
  type: string;
  text: string;
}

export interface McpToolResult {
  content: McpTextContent[];
  isError: boolean;
  structuredContent?: Record<string, unknown>;
}

export interface McpResponse {
  jsonrpc: "2.0";
  id: number;
  result?: McpToolResult;
  error?: { code: number; message: string };
}

// --- Discover / QueryProfile ---

export interface QueryProfile {
  query: string;
  task_type: string;
  task_description?: string;
  target_entity?: string;
  keywords: string[];
  sample_unit: string;
  budget?: string;
}

export interface EvaluationResult {
  cid: string;
  tcv_score: number;
  explanation: string;
  schema_fit?: number;
  quality?: number;
  community?: number;
  risk_flags?: string[];
}

// --- Wallet ---

export interface WalletBalance {
  balance: number;
  currency: string;
}

// --- Traces (Guixu backend types) ---

export interface TraceSummary {
  trace_id: string;
  trace_name: string | null;
  session_id: string | null;
  source: string;
  first_span_time: string;
  last_span_time: string;
  total_duration_ms: number;
  span_count: number;
  total_input_tokens: number;
  total_output_tokens: number;
}

export interface SpanRecord {
  trace_id: string;
  span_id: string;
  parent_span_id: string | null;
  session_id: string | null;
  span_name: string;
  span_type: string;
  source: string;
  start_time: string;
  end_time: string;
  duration_ms: number;
  attributes: Record<string, unknown>;
  input_tokens: number | null;
  output_tokens: number | null;
  model: string | null;
  error: string | null;
}

// --- Wallet Transactions ---

export interface WalletTransaction {
  id: string;
  type: string;
  amount: number;
  currency: string;
  counterparty?: string;
  dataset_cid?: string;
  timestamp: string;
  status: string;
}

// --- Approvals ---

export interface Approval {
  id: string;
  tool_name: string;
  arguments: Record<string, unknown>;
  session_id?: string;
  trace_id?: string;
  risk_level: "low" | "medium" | "high";
  status: "pending" | "approved" | "denied";
  created_at: string;
}

// --- Trace Scores ---

export interface TraceScore {
  trace_id: string;
  metric: string;
  value: number;
  label?: string;
}

// --- Memory Timeline ---

export interface MemoryEntry {
  timestamp: string;
  memory_key: string;
  operation: string;
  value_summary?: string;
  span_id?: string;
}
