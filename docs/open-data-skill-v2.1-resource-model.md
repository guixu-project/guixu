<!--
Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
SPDX-License-Identifier: Apache-2.0
-->

# Open Data Skill v2.1 Resource Model Draft

## Purpose

This draft extends Open Data Skill v2 with a `resource/download` model so Guixu
can move beyond search-only source integration.

---

## Why v2.1

Many heterogeneous data sources are not flat dataset indexes. They expose:

- datasets
- versions
- resources/files
- previews/samples
- downloadable artifacts

Guixu needs a standard resource model so skill-based sources can participate in:

- download workflows
- artifact building
- schema probing
- dataset verification

---

## Proposed Additions

### Resource Mapping

```json
{
  "resource_mapping": {
    "id": "resource_id",
    "name": "filename",
    "url": "download_url",
    "size_bytes": "size_bytes",
    "content_type": "content_type",
    "checksum": "sha256"
  }
}
```

### Download Operation

```json
{
  "operations": {
    "download": {
      "path": "/api/datasets/download",
      "method": "POST",
      "query_param": "dataset_id",
      "auth": { "kind": "bearer_env", "env": "API_TOKEN" },
      "result_path": "artifacts"
    }
  }
}
```

### Schema Probe Operation

```json
{
  "operations": {
    "schema_probe": {
      "path": "/api/datasets/schema",
      "method": "GET",
      "query_param": "dataset_id",
      "result_path": "schema.columns"
    }
  }
}
```

---

## Long-Term Direction

Open Data Skill should evolve from:

- search registry

into:

- search + lookup + resource + download + schema execution contract

So that more providers can be integrated declaratively end to end.
