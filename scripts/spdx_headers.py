#!/usr/bin/env python3
# Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
# SPDX-License-Identifier: Apache-2.0

"""Check or add short copyright and SPDX headers on first-party text files."""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path
from typing import Iterable


REPO_ROOT = Path(__file__).resolve().parent.parent
OWNER = (
    "The State Key Laboratory of Blockchain and Data Security, "
    "Zhejiang University"
)
COPYRIGHT_LINE = f"Copyright (c) 2026 {OWNER}"
SPDX_LINE = "SPDX-License-Identifier: Apache-2.0"

SKIP_EXACT = {
    "AGENTS.md",
    "Cargo.lock",
    "LICENSE",
    "NOTICE",
    "vldb-demo/package-lock.json",
    "vldb-demo/package.json",
    "vldb-demo/tsconfig.json",
    "vldb-demo/src/demo-cache.json",
}
SKIP_PREFIXES = (
    "logo/",
    "vldb-demo/assets/",
    "vldb-demo/dist/",
)

SPECIAL_STYLES = {
    ".gitignore": "line-hash",
    ".githooks/pre-commit": "line-hash",
    "Dockerfile": "line-hash",
    "rustfmt.toml": "line-hash",
}
SUFFIX_STYLES = {
    ".rs": "line-slash",
    ".toml": "line-hash",
    ".sh": "line-hash",
    ".yml": "line-hash",
    ".yaml": "line-hash",
    ".py": "line-hash",
    ".js": "block",
    ".ts": "block",
    ".tsx": "block",
    ".css": "block",
    ".html": "html",
    ".md": "html",
}


def git_paths(args: list[str]) -> list[str]:
    output = subprocess.check_output(args, cwd=REPO_ROOT, text=True)
    return [line for line in output.splitlines() if line]


def supported_style(rel_path: str) -> str | None:
    if rel_path in SKIP_EXACT or any(rel_path.startswith(prefix) for prefix in SKIP_PREFIXES):
        return None
    if rel_path in SPECIAL_STYLES:
        return SPECIAL_STYLES[rel_path]
    for suffix, style in SUFFIX_STYLES.items():
        if rel_path.endswith(suffix):
            return style
    return None


def discover_paths(explicit_paths: list[str], staged: bool) -> list[str]:
    if explicit_paths:
        candidates = explicit_paths
    elif staged:
        candidates = git_paths(["git", "diff", "--cached", "--name-only", "--diff-filter=ACMR"])
    else:
        candidates = git_paths(["git", "ls-files"])

    paths: list[str] = []
    for rel_path in candidates:
        if supported_style(rel_path) is None:
            continue
        path = REPO_ROOT / rel_path
        if path.is_file():
            paths.append(rel_path)
    return paths


def build_header(style: str) -> str:
    if style == "line-hash":
        return f"# {COPYRIGHT_LINE}\n# {SPDX_LINE}\n"
    if style == "line-slash":
        return f"// {COPYRIGHT_LINE}\n// {SPDX_LINE}\n"
    if style == "block":
        return (
            "/*\n"
            f" * {COPYRIGHT_LINE}\n"
            f" * {SPDX_LINE}\n"
            " */\n"
        )
    if style == "html":
        return (
            "<!--\n"
            f"{COPYRIGHT_LINE}\n"
            f"{SPDX_LINE}\n"
            "-->\n"
        )
    raise ValueError(f"unsupported style: {style}")


def has_expected_header(text: str) -> bool:
    leading = text[:600]
    return COPYRIGHT_LINE in leading and SPDX_LINE in leading


def insert_header(text: str, header: str) -> str:
    if text.startswith("#!"):
        newline = text.find("\n")
        if newline == -1:
            return f"{text}\n{header}\n"
        return f"{text[:newline + 1]}{header}\n{text[newline + 1:]}"

    if text.startswith("<!DOCTYPE html>"):
        newline = text.find("\n")
        if newline == -1:
            return f"{text}\n{header}\n"
        return f"{text[:newline + 1]}{header}\n{text[newline + 1:]}"

    return f"{header}\n{text}"


def check_paths(paths: Iterable[str]) -> int:
    missing: list[str] = []
    for rel_path in paths:
        text = (REPO_ROOT / rel_path).read_text(encoding="utf-8")
        if not has_expected_header(text):
            missing.append(rel_path)

    if missing:
        print("Missing or incomplete SPDX header:", file=sys.stderr)
        for rel_path in missing:
            print(f"  {rel_path}", file=sys.stderr)
        print(
            "\nRun `python3 scripts/spdx_headers.py fix` to add the standard header.",
            file=sys.stderr,
        )
        return 1

    print(f"SPDX header check passed for {len(list(paths))} file(s).")
    return 0


def fix_paths(paths: Iterable[str], stage: bool) -> int:
    updated: list[str] = []
    materialized = list(paths)
    for rel_path in materialized:
        style = supported_style(rel_path)
        if style is None:
            continue
        path = REPO_ROOT / rel_path
        text = path.read_text(encoding="utf-8")
        if has_expected_header(text):
            continue
        path.write_text(insert_header(text, build_header(style)), encoding="utf-8")
        updated.append(rel_path)

    if updated and stage:
        subprocess.check_call(["git", "add", "--", *updated], cwd=REPO_ROOT)

    if updated:
        print("Added SPDX headers to:")
        for rel_path in updated:
            print(f"  {rel_path}")
    else:
        print(f"All {len(materialized)} file(s) already had the expected SPDX header.")
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Check or add the repository's short copyright + SPDX header."
    )
    parser.add_argument("mode", choices=("check", "fix"))
    parser.add_argument("paths", nargs="*")
    parser.add_argument(
        "--staged",
        action="store_true",
        help="Operate on currently staged files instead of all tracked files.",
    )
    parser.add_argument(
        "--stage",
        action="store_true",
        help="Re-stage files that were modified in fix mode.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    paths = discover_paths(args.paths, args.staged)
    if args.mode == "check":
        return check_paths(paths)
    return fix_paths(paths, args.stage)


if __name__ == "__main__":
    raise SystemExit(main())
