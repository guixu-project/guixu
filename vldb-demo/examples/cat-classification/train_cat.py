# Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import argparse
from pathlib import Path

from ultralytics import YOLO


PROJECT_ROOT = Path(__file__).resolve().parent
DEFAULT_DATA_DIR = Path("data/cat_home_cls")
DEFAULT_PROJECT_DIR = Path("runs/cat_home")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Train a binary cat-home classifier with Ultralytics YOLO."
    )
    parser.add_argument(
        "--data",
        type=Path,
        default=PROJECT_ROOT / DEFAULT_DATA_DIR,
        help=f"Classification dataset root. Default: {DEFAULT_DATA_DIR}",
    )
    parser.add_argument(
        "--model",
        default="yolo11n-cls.pt",
        help="Pretrained classification checkpoint. Default: yolo11n-cls.pt",
    )
    parser.add_argument("--epochs", type=int, default=30, help="Training epochs.")
    parser.add_argument("--imgsz", type=int, default=640, help="Image size.")
    parser.add_argument("--batch", type=int, default=32, help="Batch size.")
    parser.add_argument(
        "--device",
        default="0",
        help="Training device, e.g. 0, 0,1, cpu. Default: 0",
    )
    parser.add_argument(
        "--project",
        type=Path,
        default=PROJECT_ROOT / DEFAULT_PROJECT_DIR,
        help=f"Ultralytics output project directory. Default: {DEFAULT_PROJECT_DIR}",
    )
    parser.add_argument(
        "--name",
        default="yolo11n_cls_cat_home",
        help="Run name under the project directory.",
    )
    parser.add_argument("--patience", type=int, default=10, help="Early stopping.")
    parser.add_argument("--workers", type=int, default=8, help="Data loader workers.")
    parser.add_argument("--seed", type=int, default=42, help="Random seed.")
    parser.add_argument(
        "--exist-ok",
        action="store_true",
        help="Allow reusing an existing Ultralytics run directory.",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    data_dir = args.data.expanduser().resolve()
    project_dir = args.project.expanduser().resolve()

    if not data_dir.is_dir():
        raise SystemExit(
            f"Classification dataset directory does not exist: {data_dir}\n"
            "Run prepare_cat_home_dataset.py first."
        )

    model = YOLO(args.model)
    results = model.train(
        data=str(data_dir),
        epochs=args.epochs,
        imgsz=args.imgsz,
        batch=args.batch,
        device=args.device,
        project=str(project_dir),
        name=args.name,
        patience=args.patience,
        workers=args.workers,
        seed=args.seed,
        exist_ok=args.exist_ok,
        pretrained=True,
        plots=True,
        amp=True,
    )

    best_weights = Path(results.save_dir) / "weights" / "best.pt"
    print(f"Best weights: {best_weights}")


if __name__ == "__main__":
    main()
