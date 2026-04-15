/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import { Search } from "lucide-react";

import { TextInput, type TextInputProps } from "./TextInput";

/**
 * A simple wrapper around the TextInput component.
 * It adds a search icon and a placeholder.
 */
export const SearchInput = ({ ...props }: TextInputProps) => {
  return (
    <TextInput
      startIcon={<Search className="size-4" />}
      placeholder="Filter..."
      {...props}
    />
  );
};
