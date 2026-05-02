<!--
Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
SPDX-License-Identifier: Apache-2.0
-->

# External Agents Module

## Adding a New Agent

1. Create a JSON config file in `agents.d/` directory
2. That's it - Guixu auto-discovers and loads it

## Config Structure

```json
{
  "id": "my-agent",
  "name": "My Agent",
  "agent_type": "custom",
  "connection": {
    "Cli": {
      "executable": "/path/to/agent",
      "args_template": ["run", "{prompt}"],
      "working_dir": null,
      "env_vars": {},
      "shell": null,
      "capture_stderr": false,
      "response_parser": "text"
    }
  },
  "default_timeout_secs": 60,
  "max_retries": 3,
  "auth": null,
  "extra": {}
}
```

## args_template Syntax

Use `{prompt}` as placeholder for user input:

```json
"args_template": ["run", "--format", "json", "{prompt}"]
```

This generates: `agent run --format json "user prompt here"`

## Response Parsers

| Parser | Description |
|--------|-------------|
| `text` | Plain text output (default) |
| `json_stream` | Newline-delimited JSON events (like OpenCode) |
| `json` | Single JSON object |
| `exit_code` | Just check exit code |

## Usage

```rust
use data_external_agents::{AgentRegistry, ExternalAgent};

// Load from agents.d/ directory
let registry = AgentRegistry::load_from_dir(Path::new("agents.d"))?;

// Get an agent by ID
let agent = registry.get("opencode-default").unwrap();

// Execute a task
let task = AgentTask::new("What is 2+2?");
let response = agent.execute_task(task).await?;
```
