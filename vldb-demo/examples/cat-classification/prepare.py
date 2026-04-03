# Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import argparse
import json
import shutil
from collections import Counter, defaultdict
from pathlib import Path

import yaml


PROJECT_ROOT = Path(__file__).resolve().parent
DEFAULT_SOURCE_DIR = Path("data/raw/catocam_full")
DEFAULT_OUTPUT_DIR = Path("data/cat_home_cls")
IMAGE_SUFFIXES = {".jpg", ".jpeg", ".png", ".bmp", ".webp"}
SPLIT_ALIASES = {
    "train": ("train",),
    "val": ("val", "valid"),
    "test": ("test",),
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Convert a detection dataset into a binary classification dataset "
            "for cat-home detection."
        )
    )
    parser.add_argument(
        "--source",
        type=Path,
        default=PROJECT_ROOT / DEFAULT_SOURCE_DIR,
        help=f"Detection dataset root directory. Default: {DEFAULT_SOURCE_DIR}",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=PROJECT_ROOT / DEFAULT_OUTPUT_DIR,
        help=f"Output classification dataset root. Default: {DEFAULT_OUTPUT_DIR}",
    )
    parser.add_argument(
        "--target-class",
        default="Cat",
        help="Detection class that means the cat is at home. Default: Cat",
    )
    parser.add_argument(
        "--positive-label",
        default="cat_home",
        help="Output class name for images containing the cat. Default: cat_home",
    )
    parser.add_argument(
        "--negative-label",
        default="cat_away",
        help="Output class name for images without the cat. Default: cat_away",
    )
    return parser.parse_args()


def load_yolo_names(data_yaml_path: Path) -> list[str]:
    with data_yaml_path.open("r", encoding="utf-8") as handle:
        data = yaml.safe_load(handle)

    names = data.get("names")
    if isinstance(names, dict):
        return [names[index] for index in sorted(names)]
    if isinstance(names, list):
        return names
    raise ValueError(f"Unsupported names format in {data_yaml_path}")


def find_split_dir(source_root: Path, split: str) -> Path | None:
    for alias in SPLIT_ALIASES[split]:
        candidate = source_root / alias
        if candidate.is_dir():
            return candidate
    return None


def list_images(images_dir: Path) -> list[Path]:
    return sorted(
        path
        for path in images_dir.rglob("*")
        if path.is_file() and path.suffix.lower() in IMAGE_SUFFIXES
    )


def unique_destination_path(destination: Path) -> Path:
    if not destination.exists():
        return destination

    stem = destination.stem
    suffix = destination.suffix
    parent = destination.parent
    index = 1
    while True:
        candidate = parent / f"{stem}_{index}{suffix}"
        if not candidate.exists():
            return candidate
        index += 1


def normalize_name(name: str) -> str:
    return name.casefold().replace(" ", "").rstrip("s")


def copy_to_class(
    image_path: Path,
    output_root: Path,
    output_split: str,
    class_name: str,
) -> None:
    destination_dir = output_root / output_split / class_name
    destination_dir.mkdir(parents=True, exist_ok=True)
    destination_path = unique_destination_path(destination_dir / image_path.name)
    shutil.copy2(image_path, destination_path)


def convert_yolo_dataset(
    source_root: Path,
    output_root: Path,
    target_class_name: str,
    positive_label: str,
    negative_label: str,
) -> dict[str, object]:
    data_yaml_path = source_root / "data.yaml"
    names = load_yolo_names(data_yaml_path)
    target_name_to_id = {normalize_name(name): index for index, name in enumerate(names)}
    target_class_id = target_name_to_id.get(normalize_name(target_class_name))
    if target_class_id is None:
        available = ", ".join(names)
        raise SystemExit(
            f"Could not find target class `{target_class_name}`. Available classes: {available}"
        )

    summary: dict[str, object] = {
        "source_format": "yolo",
        "source_root": str(source_root),
        "output_root": str(output_root),
        "target_class": target_class_name,
        "target_class_id": target_class_id,
        "detection_classes": names,
        "splits": {},
    }

    for output_split in ("train", "val", "test"):
        split_dir = find_split_dir(source_root, output_split)
        if split_dir is None:
            continue

        images_dir = split_dir / "images"
        labels_dir = split_dir / "labels"
        if not images_dir.is_dir() or not labels_dir.is_dir():
            raise SystemExit(
                f"YOLO split `{split_dir}` must contain both images/ and labels/."
            )

        counts: Counter[str] = Counter()
        for image_path in list_images(images_dir):
            label_path = labels_dir / f"{image_path.stem}.txt"
            has_target = False
            if label_path.exists():
                with label_path.open("r", encoding="utf-8") as handle:
                    for line in handle:
                        stripped = line.strip()
                        if not stripped:
                            continue
                        class_id = int(float(stripped.split()[0]))
                        if class_id == target_class_id:
                            has_target = True
                            break

            class_name = positive_label if has_target else negative_label
            copy_to_class(image_path, output_root, output_split, class_name)
            counts[class_name] += 1

        summary["splits"][output_split] = dict(counts)

    return summary


def convert_coco_dataset(
    source_root: Path,
    output_root: Path,
    target_class_name: str,
    positive_label: str,
    negative_label: str,
) -> dict[str, object]:
    summary: dict[str, object] = {
        "source_format": "coco",
        "source_root": str(source_root),
        "output_root": str(output_root),
        "target_class": target_class_name,
        "splits": {},
        "detection_classes": [],
    }
    target_id: int | None = None

    for output_split in ("train", "val", "test"):
        split_dir = find_split_dir(source_root, output_split)
        if split_dir is None:
            continue

        annotation_path = split_dir / "_annotations.coco.json"
        if not annotation_path.is_file():
            raise SystemExit(f"Missing COCO annotations: {annotation_path}")

        with annotation_path.open("r", encoding="utf-8") as handle:
            coco = json.load(handle)

        categories = coco.get("categories", [])
        if not summary["detection_classes"]:
            summary["detection_classes"] = [category["name"] for category in categories]

        category_name_to_id = {
            normalize_name(category["name"]): int(category["id"])
            for category in categories
        }
        current_target_id = category_name_to_id.get(normalize_name(target_class_name))
        if current_target_id is None:
            available = ", ".join(category["name"] for category in categories)
            raise SystemExit(
                f"Could not find target class `{target_class_name}` in {annotation_path}. "
                f"Available classes: {available}"
            )
        target_id = current_target_id

        annotations_by_image: dict[int, set[int]] = defaultdict(set)
        for annotation in coco.get("annotations", []):
            annotations_by_image[int(annotation["image_id"])].add(
                int(annotation["category_id"])
            )

        counts: Counter[str] = Counter()
        for image_record in coco.get("images", []):
            image_path = split_dir / image_record["file_name"]
            if not image_path.is_file():
                raise SystemExit(f"Missing image referenced by COCO JSON: {image_path}")

            category_ids = annotations_by_image.get(int(image_record["id"]), set())
            class_name = (
                positive_label if current_target_id in category_ids else negative_label
            )
            copy_to_class(image_path, output_root, output_split, class_name)
            counts[class_name] += 1

        summary["splits"][output_split] = dict(counts)

    summary["target_class_id"] = target_id
    return summary


def main() -> None:
    args = parse_args()
    source_root = args.source.expanduser().resolve()
    output_root = args.output.expanduser().resolve()

    if not source_root.is_dir():
        raise SystemExit(f"Source dataset directory does not exist: {source_root}")
    if output_root.exists() and any(output_root.iterdir()):
        raise SystemExit(
            f"{output_root} is not empty. Choose another --output directory."
        )

    if (source_root / "data.yaml").is_file():
        summary = convert_yolo_dataset(
            source_root=source_root,
            output_root=output_root,
            target_class_name=args.target_class,
            positive_label=args.positive_label,
            negative_label=args.negative_label,
        )
    else:
        has_coco = any(
            (find_split_dir(source_root, split) / "_annotations.coco.json").is_file()
            for split in ("train", "val", "test")
            if find_split_dir(source_root, split) is not None
        )
        if not has_coco:
            raise SystemExit(
                "Could not detect a supported detection dataset format. "
                "Expected either YOLO data.yaml or COCO _annotations.coco.json files."
            )
        summary = convert_coco_dataset(
            source_root=source_root,
            output_root=output_root,
            target_class_name=args.target_class,
            positive_label=args.positive_label,
            negative_label=args.negative_label,
        )

    metadata_path = output_root / "dataset_summary.json"
    metadata_path.parent.mkdir(parents=True, exist_ok=True)
    metadata_path.write_text(
        json.dumps(summary, ensure_ascii=False, indent=2),
        encoding="utf-8",
    )

    print(json.dumps(summary, ensure_ascii=False, indent=2))
    print(f"Classification dataset written to: {output_root}")


if __name__ == "__main__":
    main()
