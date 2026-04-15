/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import type { TraceSpan } from "@evilmartians/agent-prism-types";

import { PriceBadge } from "../PriceBadge";
import { SpanBadge } from "../SpanBadge";
import { TokensBadge } from "../TokensBadge";

interface SpanCardBagdesProps {
  data: TraceSpan;
}

export const SpanCardBadges = ({ data }: SpanCardBagdesProps) => {
  return (
    <div className="flex flex-wrap items-center justify-start gap-1">
      <SpanBadge category={data.type} />

      {typeof data.tokensCount === "number" && (
        <TokensBadge tokensCount={data.tokensCount} />
      )}

      {typeof data.cost === "number" && <PriceBadge cost={data.cost} />}
    </div>
  );
};
