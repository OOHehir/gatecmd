# gatecmd

MCP server that lets AI agents inside virtual machines execute pre-approved commands on the host — built for cases where tools like `rkdeveloptool` fail inside VMs due to USB passthrough limitations.

## Key Technologies

- **Language:** Rust (edition 2024, requires 1.75+)
- **Protocol:** Model Context Protocol (MCP) over HTTP/SSE
- **Framework:** Axum + Tokio + rmcp
- **Config:** YAML allowlist — commands, binary paths, argument patterns, per-command timeouts
- **Security:** Bearer token auth, direct execution (no shell), argument validation, path traversal prevention

## MCP Tools

| Tool | Description |
|---|---|
| `list_commands` | List all available host commands and allowed argument patterns |
| `execute_command` | Execute a whitelisted command with validated arguments |
| `list_files` | List files in the shared file root |
| `copy_file` | Copy a file within the shared file root |
| `remove_file` | Remove a file from the shared file root |

## Getting Started

**Build from source:**
```bash
cargo build --release
```

**Configure `allowed_commands.yaml`:**
```yaml
bind: "0.0.0.0:3000"
auth_token: "your-secret-token"
file_root: "/tmp/shared"
commands:
  - name: flash
    binary: /usr/bin/rkdeveloptool
    description: "Flash firmware to Rockchip device"
    allowed_args:
      - pattern: "db {file}"
        file_args: [1]
    timeout_secs: 60
```

**Add to Claude Code MCP config:**
```json
{
  "mcpServers": {
    "gatecmd": {
      "type": "http",
      "url": "http://host.docker.internal:3000/mcp",
      "headers": { "Authorization": "Bearer your-secret-token" }
    }
  }
}
```

## Security Model

Commands must be explicitly listed in the YAML config. Arguments are validated against defined patterns. Binaries are invoked directly — no shell involvement prevents injection attacks. File operations are constrained to a configured root directory to block path traversal. All requests require a bearer token.

---

Built by Owen O'Hehir — embedded Linux, IoT, Matter & Rust consulting at [electronicsconsult.com](https://electronicsconsult.com). Available for contract and consulting work.
