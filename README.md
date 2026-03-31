# gatecmd

An MCP server that lets AI agents running inside VMs execute whitelisted commands on the host machine.

Built for situations where tools like `rkdeveloptool` and `upgrade_tool` don't work properly inside a VM (USB passthrough issues, kernel module access, etc.) but the agent still needs to use them.

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
- **File root restriction** -- all file arguments must resolve under a configured `file_root` directory (e.g. a VirtualBox shared folder), preventing the agent from reading or writing arbitrary host paths
- **Bearer token auth** -- shared secret on every request
- **Per-command timeouts** -- prevents hangs

## Quick start

### 1. Configure allowed commands

Edit `allowed_commands.yaml`:

```yaml
bind: "0.0.0.0:9222"
auth_token: "your-secret-token-here"
file_root: "/path/to/shared/folder"

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

  - name: upgrade_tool
    binary: /path/to/upgrade_tool_v2.17_for_linux/upgrade_tool
    description: "Rockchip command-line firmware development tool (v2.17)"
    allowed_args:
      - pattern: "ld"
      - pattern: "db {file}"
        file_args: ["file"]
      - pattern: "ul {file}"
        file_args: ["file"]
      - pattern: "uf {file}"
        file_args: ["file"]
      - pattern: "wl {offset} {file}"
        file_args: ["file"]
      - pattern: "rl {offset} {size} {file}"
        file_args: ["file"]
      - pattern: "di -p {file}"
        file_args: ["file"]
      - pattern: "ef {file}"
        file_args: ["file"]
      - pattern: "rd {mode}"
      - pattern: "rfi"
      - pattern: "rci"
      - pattern: "pl"
      - pattern: "ssd"
    timeout_secs: 120

  - name: lsusb
    binary: /usr/bin/lsusb
    description: "List USB devices"
    allowed_args:
      - pattern: ""
      - pattern: "-v -d {vid_pid}"
    timeout_secs: 10
```

Each `pattern` defines an exact argument structure. `{name}` slots accept any value (subject to sanitization). Parameters listed in `file_args` must be absolute paths under `file_root` with no `..` traversal.

### 2. Set up the shared folder

The `file_root` directory is where the VM agent places files that it wants to reference in commands. For a VirtualBox setup:

1. Create a shared folder between the VM and host (e.g. `/home/user/vm-shared` on host, `/mnt/shared` in VM)
2. Set `file_root` in the YAML to the **host-side** path
3. The agent writes files to the shared folder from inside the VM, then passes the host-side absolute path to commands

All file arguments are validated to be under `file_root`. Attempts to reference files outside this directory are rejected.

### File management

gatecmd includes built-in file management tools (`list_files`, `copy_file`, `remove_file`) scoped to the shared directory. These let the agent stage files for flashing without depending on the VM shared mount being perfectly in sync. All paths are relative to `file_root` and subject to the same traversal protections.

### 3. Build and run on the host

```bash
cargo build --release
./target/release/gatecmd allowed_commands.yaml
```

### 4. Connect from the VM

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
- `list_commands` -- see all available commands, allowed argument patterns, and the file root path
- `execute_command` -- run a command, e.g. `{"command": "upgrade_tool", "args": "ld"}`
- `list_files` -- list files in the shared directory, e.g. `{"path": ""}` or `{"path": "images"}`
- `copy_file` -- copy a file within the shared directory, e.g. `{"src": "fw/boot.img", "dst": "staging/boot.img"}`
- `remove_file` -- delete a file from the shared directory, e.g. `{"path": "staging/old.img"}`

## Included tools

### rkdeveloptool

The standard Rockchip USB development tool (installed to `/usr/bin`). Supports device listing, boot download, loader/image flashing, and read/write by address.

### upgrade_tool (v2.17)

Rockchip's command-line firmware development tool, bundled in `upgrade_tool_v2.17_for_linux/`. Provides a superset of `rkdeveloptool` functionality including full firmware upgrade (`uf`), partition image flashing (`di`), device erase (`ef`/`el`), device info (`rfi`/`rci`/`pl`), and multi-storage/multi-device support.

See [`upgrade_tool_v2.17_for_linux/upgrade_tool_user_guide.md`](upgrade_tool_v2.17_for_linux/upgrade_tool_user_guide.md) for the full command reference (translated from the original Chinese PDF).

## Config reference

| Field | Description |
|-------|-------------|
| `bind` | Address to listen on (e.g. `0.0.0.0:9222`) |
| `auth_token` | Bearer token required for all requests |
| `file_root` | Absolute path to the directory all file arguments must be under |
| `commands[].name` | Command name (used in `execute_command` calls) |
| `commands[].binary` | Absolute path to the executable |
| `commands[].description` | Shown in `list_commands` output |
| `commands[].allowed_args[]` | List of permitted argument patterns |
| `commands[].allowed_args[].pattern` | Argument pattern (`""` for no args, `{name}` for parameter slots) |
| `commands[].allowed_args[].file_args` | Which `{name}` parameters are file paths (validated against `file_root`) |
| `commands[].timeout_secs` | Max execution time in seconds (default: 30) |

## Environment variables

- `RUST_LOG` -- set log level (e.g. `RUST_LOG=debug`)

## Building

Requires Rust 1.75+.

```bash
cargo build --release
cargo test
```
