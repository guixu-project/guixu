#!/usr/bin/env python3
# Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import re
import sys
from pathlib import Path


CJK_RE = re.compile(r"[\u3400-\u4dbf\u4e00-\u9fff\u3040-\u30ff\uac00-\ud7af]")


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: python3 scripts/check_english_commit_message.py <commit-msg-file>")
        return 2

    message_path = Path(sys.argv[1])
    message = message_path.read_text(encoding="utf-8")

    if CJK_RE.search(message):
        print("Commit message check failed. Commit messages must be written in English.")
        print(f"Offending file: {message_path}")
        return 1

    print("English commit message check passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
