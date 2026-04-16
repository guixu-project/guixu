# Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
# SPDX-License-Identifier: Apache-2.0

"""
Guixu Python SDK - Decentralized Data Platform for AI Agents

A Python SDK for interacting with the Guixu P2P data marketplace.
Provides simple APIs for searching, downloading, and publishing datasets.

Example usage:
    import guixu

    # Search for datasets
    results = guixu.search("sentiment analysis")

    # Download a dataset
    dataset = guixu.load_dataset("cid-abc123")

    # Convert to pandas DataFrame
    df = dataset.to_pandas()
"""

from guixu.client import GuixuClient
from guixu.models import Dataset, SearchResult

__version__ = "0.1.0"
__all__ = ["GuixuClient", "Dataset", "SearchResult", "search", "load_dataset"]


def search(query: str, limit: int = 10) -> list[SearchResult]:
    """Search for datasets matching the query.

    Args:
        query: Search query string (e.g., "sentiment analysis", "image classification")
        limit: Maximum number of results to return

    Returns:
        List of SearchResult objects with dataset metadata

    Example:
        >>> results = search("finance stocks")
        >>> for r in results:
        ...     print(f"{r.title}: {r.cid}")
    """
    client = GuixuClient()
    return client.search(query, limit)


def load_dataset(cid: str) -> Dataset:
    """Load a dataset by its CID.

    Args:
        cid: The Content ID (CID) of the dataset to load

    Returns:
        Dataset object ready for analysis

    Example:
        >>> ds = load_dataset("QmXxx...")
        >>> df = ds.to_pandas()
    """
    client = GuixuClient()
    return client.load_dataset(cid)


def publish(
    file_path: str,
    title: str,
    description: str | None = None,
    tags: list[str] | None = None,
    license: str = "MIT",
) -> str:
    """Publish a dataset to the P2P network.

    Args:
        file_path: Path to the dataset file (CSV, Parquet, JSON)
        title: Title of the dataset
        description: Optional description
        tags: Optional list of tags
        license: License identifier (default: MIT)

    Returns:
        The CID of the published dataset

    Example:
        >>> cid = publish("data.csv", title="My Dataset", tags=["finance"])
        >>> print(f"Published to {cid}")
    """
    client = GuixuClient()
    return client.publish(file_path, title, description, tags, license)
