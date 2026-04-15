/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import {
  OPENTELEMETRY_GENAI_ATTRIBUTES,
  OPENTELEMETRY_GENAI_MAPPINGS,
  type OpenTelemetrySpan,
  type TraceSpanCategory,
} from "@evilmartians/agent-prism-types";

import { getOpenTelemetryAttributeValue } from "./get-open-telemetry-attribute-value.js";

export function categorizeOpenTelemetryGenAI(
  span: OpenTelemetrySpan,
): TraceSpanCategory {
  const operationName = getOpenTelemetryAttributeValue(
    span,
    OPENTELEMETRY_GENAI_ATTRIBUTES.OPERATION_NAME,
  );

  if (typeof operationName === "string") {
    const category = OPENTELEMETRY_GENAI_MAPPINGS[operationName];

    if (category) return category;
  }

  return "unknown";
}
