# gatecmd

An MCP server that lets AI agents running inside VMs execute whitelisted commands on the host machine.

Built for situations where tools like `rkdeveloptool` don't work properly inside a VM (USB passthrough issues, kernel module access, etc.) but the agent still needs to use them.

## How it works

```
+-----------------------+              +----------------------------+
|   VirtualBox VM       |   HTTP/SSE   |   Host                     |
|                       |------------->|                            |
|   AI agent            |              |   gatecmd                  |
|   (Claude Code, etc.) |              |   - validates against YAML |
|                       |<-------------|   - runs command           |
|   connects to         |              |   - returns output         |
|   http://host:9222/mcp|              |                            |
+-----------------------+              +----------------------------+
```

1. Agent calls the `list_commands` MCP tool to see what's available
2. Agent calls `execute_command` with a command name and arguments
3. gatecmd checks the arguments match an allowed pattern in the YAML config
4. If valid, runs the command (no shell, direct exec) and returns stdout/stderr

## Security

- **Allowlist-only** -- commands must be explicitly listed in YAML
- **Pattern-matched args** -- arguments must match a defined pattern (e.g. `ld`, `wl {offset} {file}`)
- **No shell** -- uses `Command::new()`, never `sh -c`
- **Metacharacter rejection** -- `; | & $ \` ( ) > <` etc. are all rejected
- **Absolute binary paths** -- no PATH resolution
- **Path traversal protection** -- `..` rejected in file arguments
- **Bearer token auth** -- shared secret on every request
- **Per-command timeouts** -- prevents hangs

## Quick start

### 1. Configure allowed commands

Edit `allowed_commands.yaml`:

```yaml
bind: "0.0.0.0:9222"
auth_token: "your-secret-token-here"

commands:
  - name: rkdeveloptool
    binary: /usr/bin/rkdeveloptool
    description: "Rockchip USB development tool"
    allowed_args:
      - pattern: "ld"
      - pattern: "rd {offset} {size}"
      - pattern: "wl {offset} {file}"
        file_args: ["file"]
      - pattern: "db {file}"
        file_args: ["file"]
      - pattern: "ul {file}"
        file_args: ["file"]
    timeout_secs: 120

  - name: lsusb
    binary: /usr/bin/lsusb
    description: "List USB devices"
    allowed_args:
      - pattern: ""
      - pattern: "-v -d {vid_pid}"
    timeout_secs: 10
```

Each `pattern` defines an exact argument structure. `{name}` slots accept any value (subject to sanitization). Parameters listed in `file_args` must be absolute paths with no `..` traversal.

### 2. Build and run on the host

```bash
cargo build --release
./target/release/gatecmd allowed_commands.yaml
```

### 3. Connect from the VM

Add to your Claude Code MCP config (`~/.claude.json` or project settings):

```json
{
  "mcpServers": {
    "gatecmd": {
      "url": "http://HOST_IP:9222/mcp",
      "headers": {
        "Authorization": "Bearer your-secret-token-here"
      }
    }
  }
}
```

The agent can then use:
- `list_commands` -- see all available commands and their allowed argument patterns
- `execute_command` -- run a command, e.g. `{"command": "rkdeveloptool", "args": "ld"}`

## Config reference

| Field | Description |
|-------|-------------|
| `bind` | Address to listen on (e.g. `0.0.0.0:9222`) |
| `auth_token` | Bearer token required for all requests |
| `commands[].name` | Command name (used in `execute_command` calls) |
| `commands[].binary` | Absolute path to the executable |
| `commands[].description` | Shown in `list_commands` output |
| `commands[].allowed_args[]` | List of permitted argument patterns |
| `commands[].allowed_args[].pattern` | Argument pattern (`""` for no args, `{name}` for parameter slots) |
| `commands[].allowed_args[].file_args` | Which `{name}` parameters are file paths (validated for traversal) |
| `commands[].timeout_secs` | Max execution time in seconds (default: 30) |

## Environment variables

- `RUST_LOG` -- set log level (e.g. `RUST_LOG=debug`)

## Building

Requires Rust 1.75+.

```bash
cargo build --release
cargo test
```
