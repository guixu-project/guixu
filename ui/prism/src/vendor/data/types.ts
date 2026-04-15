/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import type {
  TraceSpan,
  TraceSpanCategory,
  TraceSpanStatus,
} from "@evilmartians/agent-prism-types";
import type { InputOutputData } from "@evilmartians/agent-prism-types";

export interface SpanAdapter<TRawDocument, TRawSpan> {
  convertRawDocumentsToSpans(
    documents: TRawDocument | TRawDocument[],
  ): TraceSpan[];

  convertRawSpansToSpanTree(spans: TRawSpan[]): TraceSpan[];

  convertRawSpanToTraceSpan(span: TRawSpan): TraceSpan;

  getSpanDuration(document: TRawSpan): number;

  getSpanCost(document: TRawSpan): number;

  getSpanTokensCount(document: TRawSpan): number;

  getSpanInputOutput(document: TRawSpan): InputOutputData;

  getSpanStatus(document: TRawSpan): TraceSpanStatus;

  getSpanCategory(document: TRawSpan): TraceSpanCategory;
}
