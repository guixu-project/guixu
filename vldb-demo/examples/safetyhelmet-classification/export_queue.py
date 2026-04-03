# Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import argparse
import csv
import json
from collections import Counter
from dataclasses import dataclass
from pathlib import Path


DEFAULT_OUTPUT_DIR = "data/proper_wear_review_queue"
DEFAULT_SOURCE_CANDIDATES = (
    Path("/data/VOC2028"),
    Path(__file__).resolve().parent / "data" / "VOC2028",
)
SPLIT_FILES = {
    "train": "train.txt",
    "val": "val.txt",
    "test": "test.txt",
}
SOURCE_LABELS = {"hat", "person"}


@dataclass(frozen=True)
class VocObject:
    label: str
    bbox: tuple[int, int, int, int]
    difficult: bool


def parse_args() -> argparse.Namespace:
    default_candidates = ", ".join(str(path) for path in DEFAULT_SOURCE_CANDIDATES)
    parser = argparse.ArgumentParser(
        description=(
            "Export VOC2028 crops for manual review before building a "
            "proper-wear classifier."
        )
    )
    parser.add_argument(
        "--source",
        type=Path,
        default=None,
        help=(
            "VOC2028 root directory. If omitted, the script tries: "
            f"{default_candidates}"
        ),
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path(DEFAULT_OUTPUT_DIR),
        help=f"Review queue output directory. Default: {DEFAULT_OUTPUT_DIR}",
    )
    parser.add_argument(
        "--pad-x",
        type=float,
        default=0.35,
        help="Horizontal padding ratio applied to each box. Default: 0.35",
    )
    parser.add_argument(
        "--pad-top",
        type=float,
        default=0.25,
        help="Top padding ratio applied to each box. Default: 0.25",
    )
    parser.add_argument(
        "--pad-bottom",
        type=float,
        default=0.55,
        help="Bottom padding ratio applied to each box. Default: 0.55",
    )
    parser.add_argument(
        "--min-size",
        type=int,
        default=20,
        help="Skip boxes whose width or height is smaller than this. Default: 20",
    )
    parser.add_argument(
        "--exclude-difficult",
        action="store_true",
        help="Skip VOC objects with difficult=1.",
    )
    return parser.parse_args()


def resolve_source_root(source: Path | None) -> Path:
    candidates = [source] if source is not None else list(DEFAULT_SOURCE_CANDIDATES)
    for candidate in candidates:
        if candidate is None:
            continue
        resolved = candidate.expanduser().resolve()
        if resolved.is_dir():
            return resolved

    candidate_text = ", ".join(str(path) for path in DEFAULT_SOURCE_CANDIDATES)
    raise SystemExit(
        "Could not find VOC2028 source directory. "
        f"Checked: {candidate_text}. "
        "Use --source to provide an explicit path."
    )


def validate_voc_layout(source_root: Path) -> tuple[Path, Path, Path]:
    annotations_dir = source_root / "Annotations"
    images_dir = source_root / "JPEGImages"
    image_sets_dir = source_root / "ImageSets" / "Main"

    if not annotations_dir.is_dir():
        raise SystemExit(f"Missing Annotations directory: {annotations_dir}")
    if not images_dir.is_dir():
        raise SystemExit(f"Missing JPEGImages directory: {images_dir}")
    if not image_sets_dir.is_dir():
        raise SystemExit(f"Missing ImageSets/Main directory: {image_sets_dir}")

    return annotations_dir, images_dir, image_sets_dir


def read_split_ids(image_sets_dir: Path, split_name: str) -> list[str]:
    split_path = image_sets_dir / SPLIT_FILES[split_name]
    if not split_path.is_file():
        raise SystemExit(f"Missing split file: {split_path}")
    return [line.strip() for line in split_path.read_text(encoding="utf-8").splitlines() if line.strip()]


def parse_voc_annotation(annotation_path: Path) -> tuple[str, list[VocObject]]:
    import xml.etree.ElementTree as ET

    tree = ET.parse(annotation_path)
    root = tree.getroot()
    filename = root.findtext("filename")
    if not filename:
        raise SystemExit(f"Annotation does not contain <filename>: {annotation_path}")

    objects: list[VocObject] = []
    for obj in root.findall("object"):
        label = (obj.findtext("name") or "").strip().casefold()
        bbox_node = obj.find("bndbox")
        if bbox_node is None:
            continue

        xmin = int(float(bbox_node.findtext("xmin", "0")))
        ymin = int(float(bbox_node.findtext("ymin", "0")))
        xmax = int(float(bbox_node.findtext("xmax", "0")))
        ymax = int(float(bbox_node.findtext("ymax", "0")))
        difficult = int(float(obj.findtext("difficult", "0"))) == 1
        objects.append(
            VocObject(
                label=label,
                bbox=(xmin, ymin, xmax, ymax),
                difficult=difficult,
            )
        )

    return filename, objects


def build_image_index(images_dir: Path) -> dict[str, Path]:
    image_index: dict[str, Path] = {}
    for image_path in images_dir.iterdir():
        if not image_path.is_file():
            continue
        image_index.setdefault(image_path.stem.casefold(), image_path)
    return image_index


def resolve_image_path(
    image_id: str,
    filename: str,
    image_index: dict[str, Path],
    images_dir: Path,
) -> Path | None:
    candidates = [
        image_index.get(image_id.casefold()),
        images_dir / filename,
        image_index.get(Path(filename).stem.casefold()),
    ]
    for candidate in candidates:
        if candidate is not None and candidate.is_file():
            return candidate
    return None


def clamp_box(
    bbox: tuple[int, int, int, int],
    image_width: int,
    image_height: int,
) -> tuple[int, int, int, int]:
    xmin, ymin, xmax, ymax = bbox
    xmin = max(0, min(xmin, image_width - 1))
    ymin = max(0, min(ymin, image_height - 1))
    xmax = max(xmin + 1, min(xmax, image_width))
    ymax = max(ymin + 1, min(ymax, image_height))
    return xmin, ymin, xmax, ymax


def expand_box(
    bbox: tuple[int, int, int, int],
    image_width: int,
    image_height: int,
    pad_x: float,
    pad_top: float,
    pad_bottom: float,
) -> tuple[int, int, int, int]:
    xmin, ymin, xmax, ymax = bbox
    box_width = xmax - xmin
    box_height = ymax - ymin
    expanded = (
        int(round(xmin - box_width * pad_x)),
        int(round(ymin - box_height * pad_top)),
        int(round(xmax + box_width * pad_x)),
        int(round(ymax + box_height * pad_bottom)),
    )
    return clamp_box(expanded, image_width=image_width, image_height=image_height)


def ensure_output_dir(output_root: Path) -> None:
    if output_root.exists() and any(output_root.iterdir()):
        raise SystemExit(f"{output_root} is not empty. Choose another --output directory.")
    output_root.mkdir(parents=True, exist_ok=True)


def export_review_queue(args: argparse.Namespace) -> dict[str, object]:
    source_root = resolve_source_root(args.source)
    output_root = args.output.expanduser().resolve()
    annotations_dir, images_dir, image_sets_dir = validate_voc_layout(source_root)
    image_index = build_image_index(images_dir)
    ensure_output_dir(output_root)

    try:
        from PIL import Image
    except ImportError as exc:
        raise SystemExit(
            "Pillow is required to export review crops. Run `pip install -r requirements.txt`."
        ) from exc

    crops_root = output_root / "crops"
    manifest_path = output_root / "review_manifest.csv"
    summary: dict[str, object] = {
        "source_root": str(source_root),
        "output_root": str(output_root),
        "manifest_path": str(manifest_path),
        "crop_padding": {
            "pad_x": args.pad_x,
            "pad_top": args.pad_top,
            "pad_bottom": args.pad_bottom,
        },
        "min_size": args.min_size,
        "exclude_difficult": bool(args.exclude_difficult),
        "splits": {},
        "ignored_labels": Counter(),
        "skipped": Counter(),
    }

    with manifest_path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(
            handle,
            fieldnames=[
                "sample_id",
                "split",
                "image_id",
                "object_index",
                "source_label",
                "crop_path",
                "review_label",
                "notes",
            ],
        )
        writer.writeheader()

        for split_name in ("train", "val", "test"):
            counts: Counter[str] = Counter()

            for image_id in read_split_ids(image_sets_dir, split_name):
                annotation_path = annotations_dir / f"{image_id}.xml"
                if not annotation_path.is_file():
                    summary["skipped"]["missing_annotation"] += 1
                    continue

                filename, objects = parse_voc_annotation(annotation_path)
                image_path = resolve_image_path(
                    image_id=image_id,
                    filename=filename,
                    image_index=image_index,
                    images_dir=images_dir,
                )
                if image_path is None:
                    summary["skipped"]["missing_image"] += 1
                    continue

                target_objects = [
                    obj
                    for obj in objects
                    if obj.label in SOURCE_LABELS and not (args.exclude_difficult and obj.difficult)
                ]
                for obj in objects:
                    if obj.label not in SOURCE_LABELS:
                        summary["ignored_labels"][obj.label] += 1

                if not target_objects:
                    summary["skipped"]["no_target_objects"] += 1
                    continue

                image = Image.open(image_path).convert("RGB")
                image_width, image_height = image.size
                try:
                    for object_index, obj in enumerate(target_objects):
                        xmin, ymin, xmax, ymax = obj.bbox
                        if xmax - xmin < args.min_size or ymax - ymin < args.min_size:
                            summary["skipped"]["too_small"] += 1
                            continue

                        crop_box = expand_box(
                            bbox=obj.bbox,
                            image_width=image_width,
                            image_height=image_height,
                            pad_x=args.pad_x,
                            pad_top=args.pad_top,
                            pad_bottom=args.pad_bottom,
                        )
                        sample_id = f"{image_id}_{object_index:03d}"
                        relative_crop_path = Path("crops") / split_name / obj.label / f"{sample_id}.jpg"
                        destination = output_root / relative_crop_path
                        destination.parent.mkdir(parents=True, exist_ok=True)
                        image.crop(crop_box).save(destination, quality=95)

                        writer.writerow(
                            {
                                "sample_id": sample_id,
                                "split": split_name,
                                "image_id": image_id,
                                "object_index": object_index,
                                "source_label": obj.label,
                                "crop_path": relative_crop_path.as_posix(),
                                "review_label": "",
                                "notes": "",
                            }
                        )
                        counts[obj.label] += 1
                finally:
                    image.close()

            summary["splits"][split_name] = dict(counts)

    summary["ignored_labels"] = dict(summary["ignored_labels"])
    summary["skipped"] = dict(summary["skipped"])
    (output_root / "export_summary.json").write_text(
        json.dumps(summary, ensure_ascii=False, indent=2),
        encoding="utf-8",
    )
    return summary


def main() -> None:
    args = parse_args()
    summary = export_review_queue(args)
    print(json.dumps(summary, ensure_ascii=False, indent=2))
    print(f"Review queue written to: {args.output.expanduser().resolve()}")


if __name__ == "__main__":
    main()
