/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import type { ComponentPropsWithRef } from "react";

import type { BadgeProps } from "./Badge";

import { Badge } from "./Badge";

export type TimestampBadgeProps = ComponentPropsWithRef<"span"> & {
  timestamp: number;
  size?: BadgeProps["size"];
};

export const TimestampBadge = ({
  timestamp,
  size,
  ...rest
}: TimestampBadgeProps) => {
  return <Badge size={size} {...rest} label={formatTimestamp(timestamp)} />;
};

function formatTimestamp(timestamp: number): string {
  return new Date(timestamp).toLocaleString();
}
