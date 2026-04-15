/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import type { TraceSpan } from "@evilmartians/agent-prism-types";

import { getDurationMs } from "./get-duration-ms.js";

export const getTimelineData = ({
  spanCard,
  minStart,
  maxEnd,
}: {
  spanCard: TraceSpan;
  minStart: number;
  maxEnd: number;
}) => {
  const startMs = +spanCard.startTime;
  const totalRange = maxEnd - minStart;
  const durationMs = getDurationMs(spanCard);
  const startPercent = ((startMs - minStart) / totalRange) * 100;
  const widthPercent = (durationMs / totalRange) * 100;

  return {
    durationMs,
    startPercent,
    widthPercent,
  };
};
