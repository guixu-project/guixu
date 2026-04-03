# Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import argparse
import csv
import json
import random
import shutil
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path


DEFAULT_MANIFEST_PATH = "data/proper_wear_review_queue/review_manifest.csv"
DEFAULT_OUTPUT_DIR = "data/proper_wear_cls"
DEFAULT_POSITIVE_LABEL = "proper_wear"
DEFAULT_NEGATIVE_LABEL = "not_proper_wear"
NEGATIVE_REVIEW_LABELS = {"not_proper_wear", "improper_wear", "no_helmet"}
POSITIVE_REVIEW_LABELS = {"proper_wear"}
VALID_SPLITS = ("train", "val", "test")
SPLIT_SEED_OFFSETS = {
    "train": 0,
    "val": 1,
    "test": 2,
}


@dataclass(frozen=True)
class ReviewedSample:
    sample_id: str
    split: str
    review_label: str
    output_label: str
    crop_path: Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Build a proper-wear / not-proper-wear classification dataset from "
            "a manually reviewed manifest."
        )
    )
    parser.add_argument(
        "--manifest",
        type=Path,
        default=Path(DEFAULT_MANIFEST_PATH),
        help=(
            "CSV manifest generated from the review queue and filled with review labels. "
            f"Default: {DEFAULT_MANIFEST_PATH}"
        ),
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path(DEFAULT_OUTPUT_DIR),
        help=f"Output classification dataset root. Default: {DEFAULT_OUTPUT_DIR}",
    )
    parser.add_argument(
        "--positive-label",
        default=DEFAULT_POSITIVE_LABEL,
        help=f"Positive output class name. Default: {DEFAULT_POSITIVE_LABEL}",
    )
    parser.add_argument(
        "--negative-label",
        default=DEFAULT_NEGATIVE_LABEL,
        help=f"Negative output class name. Default: {DEFAULT_NEGATIVE_LABEL}",
    )
    parser.add_argument(
        "--max-negative-ratio",
        type=float,
        default=3.0,
        help=(
            "Keep at most this many negative samples per positive sample in each split. "
            "Set to 0 or a negative value to disable balancing. Default: 3.0"
        ),
    )
    parser.add_argument(
        "--seed",
        type=int,
        default=42,
        help="Random seed used for negative downsampling. Default: 42",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Only validate the manifest and print summary without writing output files.",
    )
    return parser.parse_args()


def normalize_token(value: str) -> str:
    return value.strip().casefold().replace("-", "_").replace(" ", "_")


def normalize_review_label(
    review_label: str,
    positive_label: str,
    negative_label: str,
) -> tuple[str, str] | None:
    normalized = normalize_token(review_label)
    if not normalized:
        return None

    if normalized in {normalize_token(positive_label), *POSITIVE_REVIEW_LABELS}:
        return positive_label, normalized
    if normalized in {normalize_token(negative_label), *NEGATIVE_REVIEW_LABELS}:
        return negative_label, normalized
    raise ValueError(
        "Unsupported review label "
        f"`{review_label}`. Use one of: proper_wear, improper_wear, no_helmet, not_proper_wear."
    )


def ensure_output_dir(output_root: Path, dry_run: bool) -> None:
    if dry_run:
        return
    if output_root.exists() and any(output_root.iterdir()):
        raise SystemExit(f"{output_root} is not empty. Choose another --output directory.")
    output_root.mkdir(parents=True, exist_ok=True)


def resolve_crop_path(manifest_path: Path, crop_path_text: str) -> Path:
    candidate = Path(crop_path_text)
    if candidate.is_absolute():
        return candidate
    return (manifest_path.parent / candidate).resolve()


def build_destination_path(
    output_root: Path,
    split_name: str,
    class_name: str,
    sample_id: str,
    suffix: str,
) -> Path:
    safe_suffix = suffix if suffix else ".jpg"
    return output_root / split_name / class_name / f"{sample_id}{safe_suffix}"


def select_samples_for_split(
    candidates: list[ReviewedSample],
    positive_label: str,
    negative_label: str,
    max_negative_ratio: float,
    seed: int,
) -> tuple[list[ReviewedSample], dict[str, int]]:
    positives = [sample for sample in candidates if sample.output_label == positive_label]
    negatives = [sample for sample in candidates if sample.output_label == negative_label]

    selected_negatives = negatives
    balanced_out = 0
    if max_negative_ratio > 0 and positives:
        max_negatives = int(len(positives) * max_negative_ratio)
        if len(negatives) > max_negatives:
            rng = random.Random(seed)
            selected_negatives = negatives.copy()
            rng.shuffle(selected_negatives)
            selected_negatives = selected_negatives[:max_negatives]
            balanced_out = len(negatives) - len(selected_negatives)

    selected = positives + selected_negatives
    selected.sort(key=lambda sample: (sample.sample_id, str(sample.crop_path)))
    return selected, {
        positive_label: len(positives),
        negative_label: len(selected_negatives),
        "balanced_out": balanced_out,
        "available_negative": len(negatives),
    }


def load_reviewed_samples(
    manifest_path: Path,
    positive_label: str,
    negative_label: str,
) -> tuple[list[ReviewedSample], dict[str, object]]:
    if not manifest_path.is_file():
        raise SystemExit(f"Manifest does not exist: {manifest_path}")

    required_columns = {"sample_id", "split", "crop_path", "review_label"}
    skipped: Counter[str] = Counter()
    raw_review_labels: Counter[str] = Counter()
    invalid_examples: list[dict[str, str]] = []
    samples: list[ReviewedSample] = []

    with manifest_path.open("r", encoding="utf-8", newline="") as handle:
        reader = csv.DictReader(handle)
        if reader.fieldnames is None:
            raise SystemExit(f"Manifest is empty: {manifest_path}")

        missing_columns = required_columns - set(reader.fieldnames)
        if missing_columns:
            missing_text = ", ".join(sorted(missing_columns))
            raise SystemExit(f"Manifest is missing required columns: {missing_text}")

        for row in reader:
            sample_id = (row.get("sample_id") or "").strip()
            split = normalize_token(row.get("split") or "")
            crop_path_text = (row.get("crop_path") or "").strip()
            review_label_text = row.get("review_label") or ""

            if not sample_id:
                skipped["missing_sample_id"] += 1
                continue
            if split not in VALID_SPLITS:
                skipped["invalid_split"] += 1
                if len(invalid_examples) < 10:
                    invalid_examples.append(
                        {"sample_id": sample_id, "reason": f"invalid split `{row.get('split', '')}`"}
                    )
                continue
            if not crop_path_text:
                skipped["missing_crop_path"] += 1
                continue

            raw_review_labels[normalize_token(review_label_text) or "<blank>"] += 1
            try:
                normalized = normalize_review_label(
                    review_label=review_label_text,
                    positive_label=positive_label,
                    negative_label=negative_label,
                )
            except ValueError as exc:
                skipped["invalid_review_label"] += 1
                if len(invalid_examples) < 10:
                    invalid_examples.append({"sample_id": sample_id, "reason": str(exc)})
                continue

            if normalized is None:
                skipped["unlabeled"] += 1
                continue

            output_label, normalized_review_label = normalized
            crop_path = resolve_crop_path(manifest_path=manifest_path, crop_path_text=crop_path_text)
            if not crop_path.is_file():
                skipped["missing_crop_file"] += 1
                if len(invalid_examples) < 10:
                    invalid_examples.append(
                        {
                            "sample_id": sample_id,
                            "reason": f"missing crop file `{crop_path}`",
                        }
                    )
                continue

            samples.append(
                ReviewedSample(
                    sample_id=sample_id,
                    split=split,
                    review_label=normalized_review_label,
                    output_label=output_label,
                    crop_path=crop_path,
                )
            )

    diagnostics: dict[str, object] = {
        "skipped": dict(skipped),
        "raw_review_labels": dict(raw_review_labels),
        "invalid_examples": invalid_examples,
    }
    return samples, diagnostics


def convert_dataset(args: argparse.Namespace) -> dict[str, object]:
    manifest_path = args.manifest.expanduser().resolve()
    output_root = args.output.expanduser().resolve()
    ensure_output_dir(output_root, dry_run=args.dry_run)

    samples, diagnostics = load_reviewed_samples(
        manifest_path=manifest_path,
        positive_label=args.positive_label,
        negative_label=args.negative_label,
    )

    samples_by_split: dict[str, list[ReviewedSample]] = defaultdict(list)
    for sample in samples:
        samples_by_split[sample.split].append(sample)

    summary: dict[str, object] = {
        "source_format": "review_manifest",
        "manifest_path": str(manifest_path),
        "output_root": str(output_root),
        "dry_run": bool(args.dry_run),
        "class_names": [args.negative_label, args.positive_label],
        "max_negative_ratio": args.max_negative_ratio,
        "seed": args.seed,
        "review_label_mapping": {
            "proper_wear": args.positive_label,
            "improper_wear": args.negative_label,
            "no_helmet": args.negative_label,
            "not_proper_wear": args.negative_label,
        },
        "manifest_diagnostics": diagnostics,
        "splits": {},
    }

    for split_name in VALID_SPLITS:
        selected_samples, selected_counts = select_samples_for_split(
            candidates=samples_by_split.get(split_name, []),
            positive_label=args.positive_label,
            negative_label=args.negative_label,
            max_negative_ratio=args.max_negative_ratio,
            seed=args.seed + SPLIT_SEED_OFFSETS[split_name],
        )

        raw_counts: Counter[str] = Counter(sample.review_label for sample in samples_by_split.get(split_name, []))
        final_counts: Counter[str] = Counter(sample.output_label for sample in selected_samples)

        if not args.dry_run:
            for sample in selected_samples:
                destination = build_destination_path(
                    output_root=output_root,
                    split_name=split_name,
                    class_name=sample.output_label,
                    sample_id=sample.sample_id,
                    suffix=sample.crop_path.suffix.lower() or ".jpg",
                )
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(sample.crop_path, destination)

        summary["splits"][split_name] = {
            "samples": dict(final_counts),
            "review_labels": dict(raw_counts),
            "balanced_out": selected_counts["balanced_out"],
            "available_negative": selected_counts["available_negative"],
            "selected_total": len(selected_samples),
        }

    return summary


def main() -> None:
    args = parse_args()
    summary = convert_dataset(args)

    if not args.dry_run:
        output_root = args.output.expanduser().resolve()
        metadata_path = output_root / "dataset_summary.json"
        metadata_path.write_text(
            json.dumps(summary, ensure_ascii=False, indent=2),
            encoding="utf-8",
        )
        print(f"Classification dataset written to: {output_root}")

    print(json.dumps(summary, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
