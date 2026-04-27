/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

export async function apiFetch<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, init);
  if (!res.ok) {
    throw new ApiError(res.status, `${res.status} ${res.statusText}`);
  }
  return res.json() as Promise<T>;
}

let mcpId = 0;

/** Call an MCP tool via the /mcp JSON-RPC endpoint and return the parsed text result. */
export async function mcpToolCall<T = unknown>(
  name: string,
  args: Record<string, unknown>,
): Promise<T> {
  const id = ++mcpId;
  const res = await fetch("/mcp", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id,
      method: "tools/call",
      params: { name, arguments: args },
    }),
  });
  if (!res.ok) throw new ApiError(res.status, `MCP ${res.status}`);
  const json = await res.json();
  if (json.error) throw new Error(json.error.message ?? "MCP error");
  const result = json.result;
  if (result?.isError) {
    const text = result.content?.[0]?.text ?? "Tool error";
    throw new Error(text);
  }
  // Try to parse structured content first, then text
  if (result?.structuredContent) return result.structuredContent as T;
  const text = result?.content?.[0]?.text;
  if (!text) return {} as T;
  try {
    return JSON.parse(text) as T;
  } catch {
    return text as unknown as T;
  }
}
