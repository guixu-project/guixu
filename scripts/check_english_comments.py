#!/usr/bin/env python3
# Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
CJK_RE = re.compile(r"[\u3400-\u4dbf\u4e00-\u9fff\u3040-\u30ff\uac00-\ud7af]")

LINE_COMMENT_EXTENSIONS = {
    ".py",
    ".sh",
    ".yml",
    ".yaml",
    ".toml",
}

SCAN_ROOTS = (
    ROOT / "crates",
    ROOT / "scripts",
    ROOT / ".github",
    ROOT / ".githooks",
)


def contains_cjk(text: str) -> bool:
    return bool(CJK_RE.search(text))


def rust_comment_violations(path: Path) -> list[tuple[int, str]]:
    violations: list[tuple[int, str]] = []
    in_block_comment = False

    for line_no, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        cursor = 0
        while cursor < len(line):
            if in_block_comment:
                end = line.find("*/", cursor)
                if end == -1:
                    segment = line[cursor:]
                    if contains_cjk(segment):
                        violations.append((line_no, segment.strip()))
                    break
                segment = line[cursor:end]
                if contains_cjk(segment):
                    violations.append((line_no, segment.strip()))
                in_block_comment = False
                cursor = end + 2
                continue

            line_comment = line.find("//", cursor)
            block_comment = line.find("/*", cursor)

            if line_comment == -1 and block_comment == -1:
                break

            if line_comment != -1 and (block_comment == -1 or line_comment < block_comment):
                segment = line[line_comment + 2 :]
                if contains_cjk(segment):
                    violations.append((line_no, segment.strip()))
                break

            segment_start = block_comment + 2
            end = line.find("*/", segment_start)
            if end == -1:
                segment = line[segment_start:]
                if contains_cjk(segment):
                    violations.append((line_no, segment.strip()))
                in_block_comment = True
                break

            segment = line[segment_start:end]
            if contains_cjk(segment):
                violations.append((line_no, segment.strip()))
            cursor = end + 2

    return violations


def hash_comment_violations(path: Path) -> list[tuple[int, str]]:
    violations: list[tuple[int, str]] = []
    for line_no, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        comment_start = line.find("#")
        if comment_start == -1:
            continue
        segment = line[comment_start + 1 :]
        if contains_cjk(segment):
            violations.append((line_no, segment.strip()))
    return violations


def scan_file(path: Path) -> list[tuple[int, str]]:
    if path.suffix == ".rs":
        return rust_comment_violations(path)
    if path.suffix in LINE_COMMENT_EXTENSIONS:
        return hash_comment_violations(path)
    return []


def iter_source_files() -> list[Path]:
    files: list[Path] = []
    for root in SCAN_ROOTS:
        if not root.exists():
            continue
        files.extend(path for path in root.rglob("*") if path.is_file())
    return sorted(files)


def main() -> int:
    failures: list[str] = []
    for path in iter_source_files():
        violations = scan_file(path)
        for line_no, comment in violations:
            rel = path.relative_to(ROOT)
            snippet = comment[:120]
            failures.append(f"{rel}:{line_no}: non-English comment detected: {snippet}")

    if failures:
        print("English comment check failed. Replace non-English code comments with English.")
        for failure in failures:
            print(failure)
        return 1

    print("English comment check passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
