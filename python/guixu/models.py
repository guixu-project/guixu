# Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
# SPDX-License-Identifier: Apache-2.0

"""Data models for the Guixu Python SDK."""

from dataclasses import dataclass
from datetime import datetime
from typing import Any


@dataclass
class ColumnDef:
    """Column definition in a dataset schema."""

    name: str
    dtype: str
    nullable: bool = True
    description: str | None = None


@dataclass
class DatasetSchema:
    """Schema of a dataset."""

    columns: list[ColumnDef]
    row_count: int
    size_bytes: int


@dataclass
class SearchResult:
    """A dataset search result."""

    cid: str
    title: str
    description: str | None
    tags: list[str]
    schema: DatasetSchema
    data_type: str  # "tabular", "video", "image", "audio", "text"
    license: str
    provider: str
    price_amount: float | None = None
    price_currency: str = "USDC"
    created_at: datetime | None = None

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "SearchResult":
        """Create a SearchResult from a dictionary."""
        schema_data = data.get("schema", {})
        columns = [
            ColumnDef(
                name=c.get("name", ""),
                dtype=c.get("dtype", "unknown"),
                nullable=c.get("nullable", True),
                description=c.get("description"),
            )
            for c in schema_data.get("columns", [])
        ]
        schema = DatasetSchema(
            columns=columns,
            row_count=schema_data.get("row_count", 0),
            size_bytes=schema_data.get("size_bytes", 0),
        )

        price = data.get("price", {})
        price_amount = price.get("amount") if isinstance(price, dict) else None

        created_at = data.get("created_at")
        if created_at and isinstance(created_at, str):
            # Parse ISO timestamp
            try:
                created_at = datetime.fromisoformat(created_at.replace("Z", "+00:00"))
            except ValueError:
                created_at = None

        return cls(
            cid=data.get("cid", {}).get("0", data.get("cid", "")),
            title=data.get("title", ""),
            description=data.get("description"),
            tags=data.get("tags", []),
            schema=schema,
            data_type=data.get("data_type", "tabular"),
            license=data.get("license", {}).get("spdx_id", "UNKNOWN")
            if isinstance(data.get("license"), dict)
            else data.get("license", "UNKNOWN"),
            provider=data.get("provider", {}).get("0", data.get("provider", ""))
            if isinstance(data.get("provider"), dict)
            else data.get("provider", ""),
            price_amount=price_amount,
            created_at=created_at,
        )


@dataclass
class Dataset:
    """A loaded dataset ready for analysis."""

    cid: str
    title: str
    description: str | None
    schema: DatasetSchema
    _data: dict[str, Any] | None = None
    _file_path: str | None = None

    def to_pandas(self):
        """Convert the dataset to a pandas DataFrame.

        Returns:
            pandas.DataFrame: The dataset as a DataFrame

        Raises:
            ImportError: If pandas is not installed
        """
        pandas = __import__("pandas", fromlist=["pandas"])
        if self._data is not None:
            # Data was loaded inline
            return pandas.DataFrame(self._data)
        elif self._file_path:
            # Load from file path
            path = self._file_path
            if path.endswith(".parquet"):
                return pandas.read_parquet(path)
            elif path.endswith(".csv"):
                return pandas.read_csv(path)
            elif path.endswith(".json"):
                return pandas.read_json(path)
            else:
                raise ValueError(f"Unsupported file format: {path}")
        else:
            raise ValueError("No data available in dataset")

    def to_polars(self):
        """Convert the dataset to a Polars DataFrame.

        Returns:
            polars.DataFrame: The dataset as a Polars DataFrame

        Raises:
            ImportError: If polars is not installed
        """
        polars = __import__("polars", fromlist=["polars"])
        if self._data is not None:
            return polars.DataFrame(self._data)
        elif self._file_path:
            path = self._file_path
            if path.endswith(".parquet"):
                return polars.read_parquet(path)
            elif path.endswith(".csv"):
                return polars.read_csv(path)
            elif path.endswith(".json"):
                return polars.read_json(path)
            else:
                raise ValueError(f"Unsupported file format: {path}")
        else:
            raise ValueError("No data available in dataset")

    @classmethod
    def from_search_result(cls, result: SearchResult) -> "Dataset":
        """Create a Dataset from a SearchResult (without loading data)."""
        return cls(
            cid=result.cid,
            title=result.title,
            description=result.description,
            schema=result.schema,
        )
