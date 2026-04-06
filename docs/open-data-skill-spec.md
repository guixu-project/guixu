# Open Data Skill Spec v2 Draft

## Purpose

`Open Data Skill` is Guixu's declarative source-integration standard.

Its purpose is to let Guixu onboard large numbers of heterogeneous data sources
through skill files rather than invasive Rust source changes.

This draft upgrades the original search-first spec into a broader source model
covering:

- provider identity
- capabilities
- executable operations
- normalization mappings
- governance metadata
- validation and observability expectations

---

## Design Goals

An Open Data Skill should:

1. let new sources be registered by file rather than code
2. support both built-in/native and fully declarative providers
3. describe heterogeneous HTTP source behavior in a uniform way
4. provide enough metadata for planning, policy, and governance decisions
5. scale operationally to very large numbers of sources

---

## Versioning

Each skill file must declare a `spec_version`.

Current draft target:

```json
"spec_version": "2.0"
```

Compatibility rules:

- `1.x` is legacy/search-first
- `2.x` introduces capabilities, operations, and governance
- unknown major versions must be rejected by the runtime

Recommended rollout policy:

- support `1.x` and `2.x` during migration
- all newly authored skills should target `2.0`

---

## File Locations

Built-in skills:

- `crates/search/skills/builtin/`

External skills:

- directories listed in `GUIXU_OPEN_DATA_SKILL_DIRS`

Example:

```bash
GUIXU_OPEN_DATA_SKILL_DIRS=/opt/guixu/skills:/workspace/data-skills
```

Current runtime format:

- JSON files only

---

## Top-Level Schema

```json
{
  "spec_version": "2.0",
  "id": "example_registry",
  "name": "Example Registry",
  "description": "Search an HTTP dataset registry.",
  "source": "open_data_skill",
  "tags": ["registry", "http"],
  "enabled": true,
  "capabilities": {
    "search": true,
    "lookup": false,
    "download": false,
    "schema_probe": false,
    "sample_preview": false,
    "license_lookup": false
  },
  "governance": {
    "trust_tier": "community",
    "rate_limit_hint": {
      "requests_per_minute": 60,
      "burst": 10
    },
    "provenance_hint": "provider-managed registry",
    "compliance_hint": "review source license before commercial use"
  },
  "provider": {
    "kind": "http_search",
    "base_url": "https://example.com",
    "operations": {
      "search": {
        "path": "/api/datasets/search",
        "method": "GET",
        "query_param": "q",
        "limit_param": "limit",
        "static_params": {
          "type": "dataset"
        },
        "headers": {
          "Accept": "application/json"
        },
        "auth": {
          "kind": "none"
        },
        "result_path": "results.items",
        "pagination": {
          "kind": "offset_limit",
          "page_param": "offset",
          "size_param": "limit",
          "start": 0,
          "step": 50,
          "max_pages": 3
        }
      }
    },
    "item_mapping": {
      "title": "title",
      "description": "description",
      "id": "id",
      "size_bytes": "size_bytes"
    }
  }
}
```

---

## Top-Level Fields

| Field | Type | Required | Meaning |
|------|------|----------|---------|
| `spec_version` | string | yes | Skill spec version |
| `id` | string | yes | Stable provider/skill identifier |
| `name` | string | yes | Human-readable provider name |
| `description` | string | yes | Provider description |
| `source` | string | yes | Guixu source classification |
| `tags` | string[] | no | Search labels |
| `enabled` | bool | yes | Runtime load toggle |
| `capabilities` | object | no | Supported operations |
| `governance` | object | no | Trust/rate/compliance metadata |
| `provider` | object | yes | Execution contract |

---

## Source Classification

Current runtime still supports legacy `source` values, but the strategic
direction is to combine them with a more open provider model.

Recommended interpretation:

- `source` = compatibility label
- `provider.id` = stable unique provider identity
- `source_family` = inferred or declared higher-level category

Typical source families:

- `marketplace`
- `academic`
- `web_registry`
- `db_catalog`
- `decentralized`
- `local`
- `custom`

---

## Capabilities

Capabilities let Guixu plan workflows without guessing what a provider supports.

```json
{
  "capabilities": {
    "search": true,
    "lookup": false,
    "download": false,
    "schema_probe": false,
    "sample_preview": false,
    "license_lookup": false
  }
}
```

### Capability Fields

| Field | Meaning |
|------|---------|
| `search` | Provider supports dataset search |
| `lookup` | Provider supports detailed item lookup |
| `download` | Provider supports artifact download |
| `schema_probe` | Provider supports schema introspection |
| `sample_preview` | Provider supports sample preview |
| `license_lookup` | Provider supports explicit license lookup |

---

## Governance

Governance metadata lets Guixu reason about trust, compliance, and scheduling.

```json
{
  "governance": {
    "trust_tier": "community",
    "rate_limit_hint": {
      "requests_per_minute": 60,
      "burst": 10
    },
    "provenance_hint": "provider-managed registry",
    "compliance_hint": "review source license before commercial use"
  }
}
```

### `trust_tier`

Supported values:

- `unknown`
- `community`
- `verified`
- `first_party`

### `rate_limit_hint`

```json
{
  "requests_per_minute": 60,
  "burst": 10
}
```

This is advisory metadata for planner/scheduler behavior.

---

## Provider Kinds

## 1. `native_adapter`

Compatibility bridge for existing Rust adapters.

```json
{
  "provider": {
    "kind": "native_adapter",
    "adapter": "kaggle"
  }
}
```

Use only when the provider is not yet representable declaratively.

## 2. `http_search`

Declarative provider driven by executable HTTP operations.

```json
{
  "provider": {
    "kind": "http_search",
    "base_url": "https://example.com",
    "operations": {
      "search": { ... },
      "lookup": { ... },
      "download": { ... },
      "schema_probe": { ... }
    },
    "item_mapping": { ... }
  }
}
```

Current runtime executes only `operations.search`, but the spec reserves the
full operation model now so new source integrations don't need another schema rewrite.

---

## Operations

An operation is an executable action definition.

Current operation names:

- `search`
- `lookup`
- `download`
- `schema_probe`

### Operation Schema

```json
{
  "path": "/api/datasets/search",
  "method": "GET",
  "query_param": "q",
  "limit_param": "limit",
  "static_params": {
    "type": "dataset"
  },
  "headers": {
    "Accept": "application/json"
  },
  "auth": {
    "kind": "none"
  },
  "result_path": "results.items",
  "pagination": {
    "kind": "offset_limit",
    "page_param": "offset",
    "size_param": "limit",
    "start": 0,
    "step": 50,
    "max_pages": 3
  }
}
```

### Operation Fields

| Field | Type | Meaning |
|------|------|---------|
| `path` | string | Endpoint path |
| `method` | string | `GET` or `POST` |
| `query_param` | string | Search query field |
| `limit_param` | string | Requested size field |
| `static_params` | object | Constant params/body values |
| `headers` | object | Static headers |
| `auth` | object | Authentication config |
| `result_path` | string | Dot-path to result array |
| `pagination` | object | Pagination strategy |

---

## Authentication

Supported auth kinds in the current runtime:

### `none`

```json
{ "kind": "none" }
```

### `bearer_env`

```json
{
  "kind": "bearer_env",
  "env": "HF_TOKEN"
}
```

Runtime behavior:

- read token from environment
- attach `Authorization: Bearer <token>`

### `header_env`

```json
{
  "kind": "header_env",
  "header": "X-API-Key",
  "env": "EXAMPLE_API_KEY"
}
```

Runtime behavior:

- read secret from environment
- attach it to the configured header

### Recommended future auth kinds

- `basic_env`
- `oauth_client_credentials`
- `signed_request`
- `cookie_env`

---

## Pagination

Supported pagination kinds in the current runtime:

### `none`

Single request.

### `offset_limit`

```json
{
  "kind": "offset_limit",
  "page_param": "offset",
  "size_param": "limit",
  "start": 0,
  "step": 50,
  "max_pages": 3
}
```

### `page_number`

```json
{
  "kind": "page_number",
  "page_param": "page",
  "size_param": "page_size",
  "start": 1,
  "max_pages": 5
}
```

### Recommended future pagination kinds

- `cursor`
- `next_url`
- `continuation_token`

---

## Normalization Mapping

`item_mapping` converts heterogeneous provider responses into Guixu's normalized
`SearchResult` shape.

Current runtime fields:

| Field | Meaning |
|------|---------|
| `title` | Field containing item title |
| `description` | Field containing item description |
| `id` | Stable item identifier |
| `size_bytes` | Optional size field |

Recommended v2 expansion:

- `tags`
- `license`
- `created_at`
- `download_count`
- `provider_name`
- `data_type`
- `resource_count`

---

## Provider Model and Governance Output

Runtime normalization should attach:

- `provider_meta.provider_id`
- `provider_meta.source_family`
- `provider_meta.labels`
- `governance.trust_tier`
- `governance.rate_limit_hint`
- `governance.provenance_hint`
- `governance.compliance_hint`

This makes Open Data Skill useful not just for transport, but also for planning,
ranking, and policy decisions.

---

## Validation Rules

At minimum, runtimes should validate:

- `spec_version` major compatibility
- non-empty `id`
- required provider fields
- required `operations.search.path` for `http_search`
- supported auth and pagination kinds

Recommended future validation:

- duplicate skill IDs
- invalid mapping fields
- invalid `result_path`
- auth config completeness
- operation/capability mismatch

---

## Observability Requirements

Each skill execution should emit structured metadata suitable for logs/metrics:

- `skill_id`
- `provider_kind`
- `operation`
- request method
- endpoint path
- pagination pages fetched
- result count
- failure reason
- auth mode used (not secret values)

---

## Runtime Loading Rules

Guixu loads skills from:

1. `crates/search/skills/builtin/`
2. `GUIXU_OPEN_DATA_SKILL_DIRS`

Loading rules:

- ignore non-JSON files
- reject invalid JSON
- reject unsupported major `spec_version`
- skip disabled skills
- skip skills in disabled runtime list

---

## Migration Strategy

### Phase 1

- represent all existing built-in sources as built-in skills
- keep `native_adapter` for compatibility

### Phase 2

- prefer `http_search` for new source onboarding
- stop adding new hardcoded source registrations

### Phase 3

- add runtime support for `lookup`, `download`, and `schema_probe`
- enrich normalization mapping
- expand auth and pagination kinds

### Phase 4

- reserve native adapters only for genuinely complex providers

---

## Long-Term Direction

The long-term architecture is:

- source registration by skill file
- execution by generic operation executor
- normalization by declarative mapping
- planning and governance informed by capabilities and metadata

In short:

> Adding a new data source should usually mean adding a skill file, not editing Rust code.
