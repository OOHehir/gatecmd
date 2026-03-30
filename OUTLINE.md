# gatecmd — Host Command Proxy MCP Server

## Problem

AI agents running inside VirtualBox VMs cannot reliably access host USB devices
or certain host-only tools (e.g. `rkdeveloptool` for Rockchip flashing). We need
a secure bridge that lets the agent invoke specific, whitelisted commands on the
host machine.

## Solution

A Rust MCP server running on the **host**, exposing whitelisted shell commands as
MCP tools over Streamable HTTP (SSE). The agent in the VM connects to the host's
IP and calls tools like `execute_command`.

## Architecture

```
┌──────────────────────────┐              ┌─────────────────────────────────┐
│   VirtualBox VM          │   HTTP/SSE   │   Host (this app)               │
│                          │─────────────>│                                 │
│   AI Agent               │              │   gatecmd                       │
│   (Claude Code, etc.)    │              │   ├─ MCP Streamable HTTP        │
│                          │<─────────────│   ├─ YAML allowlist             │
│   Connects to            │              │   ├─ Command sanitizer          │
│   http://host:9222/mcp   │              │   └─ tokio::process::Command    │
└──────────────────────────┘              └─────────────────────────────────┘
```

## Components

### 1. YAML Configuration (`allowed_commands.yaml`)

```yaml
bind: "0.0.0.0:9222"
auth_token: "change-me-to-a-secret"

commands:
  - name: rkdeveloptool
    binary: /usr/bin/rkdeveloptool
    description: "Rockchip USB development tool"
    allowed_args:
      - pattern: "ld"                    # list devices
      - pattern: "rd {offset} {size}"    # read flash
      - pattern: "wl {offset} {file}"    # write at offset
        file_args: [file]                # validate these args are real paths
      - pattern: "db {file}"             # download boot
        file_args: [file]
      - pattern: "ul {file}"             # upgrade loader
        file_args: [file]
    timeout_secs: 120

  - name: lsusb
    binary: /usr/bin/lsusb
    description: "List USB devices"
    allowed_args:
      - pattern: ""                      # no args
      - pattern: "-v -d {vid}:{pid}"     # verbose for specific device
    timeout_secs: 10
```

**Key design decisions:**
- `binary`: absolute path, never resolved via PATH
- `allowed_args`: each entry is a pattern; `{name}` are parameter slots
- `file_args`: which parameters must be valid filesystem paths
- `timeout_secs`: per-command timeout to prevent hangs

### 2. Command Sanitizer (`src/sanitizer.rs`)

Responsibilities:
- Match incoming command + args against allowlist patterns
- Reject shell metacharacters: `; | & $ \` ( ) { } > < \n \r`
- Reject path traversal in file args (`..`)
- Ensure binary path matches config exactly
- No shell invocation — always `Command::new(binary).args(&[...])`

### 3. MCP Server (`src/server.rs`)

Uses `rmcp` crate with Streamable HTTP transport:
- **Tool: `list_commands`** — returns available commands and their allowed arg patterns
- **Tool: `execute_command`** — takes `command` name + `args` string, validates, executes, returns stdout/stderr
- Auth via `Authorization: Bearer <token>` header check

### 4. Auth Middleware (`src/auth.rs`)

Simple bearer token check on all MCP requests. Token from config file.

## Security Model

1. **Allowlist-only**: only commands explicitly listed in YAML can run
2. **Pattern matching**: args must match a defined pattern exactly
3. **No shell**: `Command::new()` — no `sh -c`, no metacharacter interpretation
4. **Absolute paths**: binaries referenced by full path
5. **Timeouts**: every command has a deadline
6. **Bearer auth**: shared secret prevents unauthorized access
7. **File arg validation**: parameters marked as file args are checked for path traversal
8. **Bind address**: defaults to `0.0.0.0` for VM access, but configurable

## Crate Dependencies

| Crate | Purpose |
|-------|---------|
| `rmcp` | MCP protocol server (streamable HTTP) |
| `axum` | HTTP framework (used by rmcp) |
| `tokio` | Async runtime |
| `serde` / `serde_yaml` | Config parsing |
| `schemars` | JSON Schema for MCP tool params |
| `tracing` | Structured logging |
| `anyhow` | Error handling |

## File Structure

```
gatecmd/
├── Cargo.toml
├── allowed_commands.yaml      # example config
├── src/
│   ├── main.rs                # entry point, server startup
│   ├── config.rs              # YAML config types + loader
│   ├── sanitizer.rs           # command validation + sanitization
│   ├── server.rs              # MCP server handler + tools
│   └── auth.rs                # bearer token middleware
```
