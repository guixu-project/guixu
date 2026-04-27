/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import type { ComponentPropsWithRef } from "react";

import { ChevronsUpDown, ChevronsDownUp } from "lucide-react";

import { IconButton } from "./IconButton";

export type SpanCardExpandAllButtonProps = ComponentPropsWithRef<"button"> & {
  onExpandAll: () => void;
};

export type SpanCardCollapseAllButtonProps = ComponentPropsWithRef<"button"> & {
  onCollapseAll: () => void;
};

export const ExpandAllButton = ({
  onExpandAll,
  ...rest
}: SpanCardExpandAllButtonProps) => {
  return (
    <IconButton
      size="6"
      onClick={onExpandAll}
      aria-label="Expand all"
      icon={<ChevronsUpDown className="size-3.5" />}
      {...rest}
    />
  );
};

export const CollapseAllButton = ({
  onCollapseAll,
  ...rest
}: SpanCardCollapseAllButtonProps) => {
  return (
    <IconButton
      size="6"
      onClick={onCollapseAll}
      aria-label="Collapse all"
      icon={<ChevronsDownUp className="size-3.5" />}
      {...rest}
    />
  );
};