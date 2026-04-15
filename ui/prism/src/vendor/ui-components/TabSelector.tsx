/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import { type ReactElement } from "react";

import { type TabItem, Tabs } from "./Tabs";

export interface TabSelectorProps<T extends string> {
  items: TabItem<T>[];
  value: T;
  onValueChange: (value: T) => void;
  defaultValue?: T;
  theme?: "underline" | "pill";
  className?: string;
  onClick?: (event: React.MouseEvent) => void;
}

export const TabSelector = <T extends string>({
  items,
  value,
  onValueChange,
  defaultValue,
  theme = "underline",
  className,
  onClick,
}: TabSelectorProps<T>): ReactElement => {
  return (
    <Tabs<T>
      items={items}
      value={value}
      onValueChange={onValueChange}
      defaultValue={defaultValue}
      theme={theme}
      className={className}
      onClick={onClick}
    />
  );
};
