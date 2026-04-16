# Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
# SPDX-License-Identifier: Apache-2.0

"""
Guixu MCP Client - Python SDK for Guixu P2P Data Platform

This module provides a Python client for the Guixu MCP server.
The MCP server exposes tools for dataset search, download, and publishing.

By default, the client connects to the local MCP server via STDIO.
Set GUIXU_MCP_URL environment variable to connect via HTTP.

Example:
    client = GuixuClient()
    results = client.search("machine learning datasets")
    dataset = client.load_dataset(results[0].cid)
"""

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

from guixu.models import Dataset, SearchResult


class GuixuClient:
    """Python client for the Guixu P2P Data Platform MCP server."""

    def __init__(
        self,
        mcp_command: str | None = None,
        mcp_args: list[str] | None = None,
    ):
        """Initialize the Guixu MCP client.

        Args:
            mcp_command: Path to the MCP server binary. Defaults to "data-node".
            mcp_args: Additional arguments to pass to the MCP server.
        """
        self.mcp_command = mcp_command or os.environ.get("GUIXU_MCP_COMMAND", "data-node")
        self.mcp_args = mcp_args or []

    def _call_mcp_tool(self, tool_name: str, arguments: dict[str, Any]) -> dict[str, Any]:
        """Call an MCP tool and return the result.

        Args:
            tool_name: Name of the MCP tool to call
            arguments: Dictionary of tool arguments

        Returns:
            Dictionary containing the tool result

        Raises:
            RuntimeError: If the MCP call fails
        """
        # Build the MCP JSON-RPC request
        request = {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": arguments,
            },
        }

        # Try to use HTTP if GUIXU_MCP_URL is set
        mcp_url = os.environ.get("GUIXU_MCP_URL")
        if mcp_url:
            return self._call_http(mcp_url, request)

        # Use STDIO to communicate with the MCP server
        return self._call_stdio(request)

    def _call_stdio(self, request: dict[str, Any]) -> dict[str, Any]:
        """Call MCP tool via STDIO communication.

        Args:
            request: JSON-RPC request dictionary

        Returns:
            JSON-RPC response dictionary
        """
        # Run the MCP server as a subprocess
        cmd = [self.mcp_command] + self.mcp_args + ["mcp", "run"]

        try:
            process = subprocess.Popen(
                cmd,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            # Send the request
            request_json = json.dumps(request) + "\n"
            stdout, stderr = process.communicate(input=request_json, timeout=60)

            if process.returncode != 0:
                raise RuntimeError(f"MCP server error: {stderr}")

            # Parse the response (one JSON object per line)
            for line in stdout.strip().split("\n"):
                if line.startswith("{"):
                    response = json.loads(line)
                    if "result" in response:
                        return response["result"]
                    elif "error" in response:
                        raise RuntimeError(f"MCP error: {response['error']}")

            raise RuntimeError("No valid response from MCP server")

        except subprocess.TimeoutExpired:
            process.kill()
            raise RuntimeError("MCP request timed out")
        except FileNotFoundError:
            raise RuntimeError(
                f"MCP server not found: {self.mcp_command}. "
                "Is data-node installed and in your PATH?"
            )

    def _call_http(self, url: str, request: dict[str, Any]) -> dict[str, Any]:
        """Call MCP tool via HTTP communication.

        Args:
            url: HTTP URL of the MCP server
            request: JSON-RPC request dictionary

        Returns:
            JSON-RPC response dictionary
        """
        requests = __import__("requests")

        headers = {"Content-Type": "application/json"}
        response = requests.post(
            url,
            json=request,
            headers=headers,
            timeout=60,
        )

        if response.status_code != 200:
            raise RuntimeError(f"HTTP error: {response.status_code} - {response.text}")

        result = response.json()
        if "error" in result:
            raise RuntimeError(f"MCP error: {result['error']}")

        return result.get("result", {})

    def search(self, query: str, limit: int = 10) -> list[SearchResult]:
        """Search for datasets matching the query.

        Args:
            query: Search query string
            limit: Maximum number of results

        Returns:
            List of SearchResult objects
        """
        result = self._call_mcp_tool("dataset_search", {"query": query, "limit": limit})

        # Parse the result - it may be a JSON string or already parsed
        if isinstance(result, str):
            parsed = json.loads(result)
        else:
            parsed = result

        # Extract results array from the nested structure
        if isinstance(parsed, dict):
            # Try common patterns
            if "results" in parsed:
                items = parsed["results"]
            elif "content" in parsed:
                # MCP content wrapper
                content = parsed["content"]
                if isinstance(content, list) and len(content) > 0:
                    first = content[0]
                    if isinstance(first, dict) and "text" in first:
                        items = json.loads(first["text"])
                    else:
                        items = content
                else:
                    items = []
            else:
                items = parsed if isinstance(parsed, list) else []
        else:
            items = parsed if isinstance(parsed, list) else []

        return [SearchResult.from_dict(item) for item in items]

    def load_dataset(self, cid: str, rows: int = 1000) -> Dataset:
        """Load a dataset by its CID.

        Args:
            cid: The Content ID of the dataset
            rows: Maximum number of rows to preview

        Returns:
            Dataset object with metadata and preview data
        """
        # First lookup the dataset metadata
        lookup_result = self._call_mcp_tool("dataset_lookup", {"cid": cid})

        # Parse lookup result
        if isinstance(lookup_result, str):
            metadata_list = json.loads(lookup_result)
        else:
            metadata_list = lookup_result if isinstance(lookup_result, list) else []

        if not metadata_list:
            raise ValueError(f"Dataset not found: {cid}")

        metadata = metadata_list[0] if isinstance(metadata_list[0], dict) else {}

        # Get a preview/sample of the data
        sample_result = self._call_mcp_tool("dataset_query", {"cid": cid, "rows": rows})

        # Create dataset with metadata
        schema_data = metadata.get("schema", {})
        from guixu.models import ColumnDef, DatasetSchema

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

        dataset = Dataset(
            cid=cid,
            title=metadata.get("title", "Untitled"),
            description=metadata.get("description"),
            schema=schema,
        )

        # Parse sample data if available
        if isinstance(sample_result, str):
            sample_data = json.loads(sample_result)
        else:
            sample_data = sample_result if isinstance(sample_result, dict) else {}

        if "preview_data" in sample_data:
            # Base64 decode preview data
            import base64

            preview_b64 = sample_data["preview_data"]
            if preview_b64:
                preview_bytes = base64.b64decode(preview_b64)
                preview_text = preview_bytes.decode("utf-8")
                # Parse CSV preview into data dict
                import csv
                import io

                reader = csv.DictReader(io.StringIO(preview_text))
                dataset._data = list(reader)

        return dataset

    def publish(
        self,
        file_path: str,
        title: str,
        description: str | None = None,
        tags: list[str] | None = None,
        license: str = "MIT",
    ) -> str:
        """Publish a dataset to the P2P network.

        Args:
            file_path: Path to the dataset file
            title: Title of the dataset
            description: Optional description
            tags: Optional list of tags
            license: License identifier (default: MIT)

        Returns:
            The CID of the published dataset
        """
        if not Path(file_path).exists():
            raise FileNotFoundError(f"File not found: {file_path}")

        args = {
            "title": title,
            "file_path": str(Path(file_path).absolute()),
            "license": license,
        }
        if description:
            args["description"] = description
        if tags:
            args["tags"] = tags

        result = self._call_mcp_tool("dataset_publish", args)

        # Parse result
        if isinstance(result, str):
            parsed = json.loads(result)
        else:
            parsed = result if isinstance(result, dict) else {}

        # Extract CID from result
        cid = parsed.get("cid", "")
        if not cid:
            # Try to find CID in nested structures
            if "content" in parsed:
                content = parsed["content"]
                if isinstance(content, list) and len(content) > 0:
                    text = content[0].get("text", "") if isinstance(content[0], dict) else str(content[0])
                    parsed = json.loads(text)
                    cid = parsed.get("cid", "")

        return cid
