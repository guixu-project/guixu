/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

export function convertNanoTimestampToDate(nanoString: string): Date {
  const nanoseconds = BigInt(nanoString);
  const milliseconds = Number(nanoseconds / BigInt(1_000_000));

  return new Date(milliseconds);
}
