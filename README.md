# RustyClaw 🦀🦞

A super-lightweight, super-capable agentic tool with improved security versus OpenClaw.

<p align="center">
  <img src="logo.svg" alt="RustyClaw Logo" width="200"/>
</p>

## ✅ Feature Parity Status

**RustyClaw has achieved full feature parity with OpenClaw!**

| Category | Status | Details |
|----------|--------|---------|
| **Tools** | ✅ Complete | 30 tools implemented |
| **Skills** | ✅ Complete | Load, gate, inject into prompts |
| **Messengers** | ✅ Complete | Webhook, Console, Discord, Telegram |
| **Streaming** | ✅ Complete | OpenAI + Anthropic SSE |
| **Gateway** | ✅ Complete | WebSocket, auth, heartbeat |
| **TUI** | ✅ Complete | 12+ slash commands |
| **Secrets** | ✅ Complete | Vault + TOTP + policies |
| **Multi-session** | ✅ Complete | Spawn, list, send, history |

### Tools (30 total)

| Category | Tools |
|----------|-------|
| **File** | read_file, write_file, edit_file, list_directory, search_files, find_files |
| **Runtime** | execute_command, process |
| **Web** | web_fetch, web_search |
| **Memory** | memory_search, memory_get |
| **Scheduling** | cron |
| **Sessions** | sessions_list, sessions_spawn, sessions_send, sessions_history, session_status, agents_list |
| **Editing** | apply_patch |
| **Secrets** | secrets_list, secrets_get, secrets_store |
| **System** | gateway, message, tts |
| **Media** | image |
| **Devices** | nodes |
| **Browser** | browser |
| **Canvas** | canvas |

## Features

- **Written in Rust**: High-performance, memory-safe implementation
- **OpenClaw Compatible**: Drop-in replacement with same tools and skills
- **30 Agentic Tools**: Full tool coverage for file, web, memory, sessions, and more
- **Skills Support**: OpenClaw/AgentSkills compatible with gating
- **SOUL.md**: Configurable agent personality and behavior
- **Secure Secrets Storage**: Encrypted vault with TOTP 2FA and policies
- **TUI Interface**: Terminal UI with slash commands and tab completion
- **Multi-Provider**: OpenAI, Anthropic, Google, GitHub Copilot, xAI, Ollama, OpenRouter
- **Streaming**: Real-time token delivery from providers
- **Messenger Backends**: Webhook, Console, Discord, Telegram

## Installation

### Prerequisites

- Rust 1.75 or later
- Cargo (comes with Rust)

### Building from Source

```bash
git clone https://github.com/rexlunae/RustyClaw.git
cd RustyClaw
cargo build --release
```

The binary will be available at `target/release/rustyclaw`.

## Quick Start

### Run the TUI

```bash
rustyclaw tui
```

### Run the Gateway

```bash
rustyclaw gateway start
```

### Send a One-Shot Command

```bash
rustyclaw command "What time is it?"
```

### Check Status

```bash
rustyclaw status
```

## CLI Commands

```
rustyclaw
├── setup          # Initialize config + workspace
├── onboard        # Interactive setup wizard
├── configure      # Configuration wizard
├── config         # Config get/set/unset
├── doctor         # Health checks + fixes
├── tui            # Launch terminal UI
├── command        # Send one-shot message
├── status         # Show system status
├── gateway        # Gateway management
│   ├── start
│   ├── stop
│   ├── restart
│   └── status
└── skills         # Skill management
    ├── list
    └── enable/disable
```

## TUI Slash Commands

| Command | Description |
|---------|-------------|
| `/help` | Show available commands |
| `/clear` | Clear message history |
| `/provider` | Change AI provider |
| `/model` | Change model |
| `/gateway` | Gateway connection status |
| `/secrets` | Manage secrets |
| `/skills` | List loaded skills |
| `/status` | Show session status |
| `/quit` | Exit the TUI |

## Configuration

Configuration lives at `~/.rustyclaw/config.toml`:

```toml
[gateway]
bind = "127.0.0.1:18789"
token = "your-secret-token"

[model]
provider = "anthropic"
model = "claude-sonnet-4-20250514"

[secrets]
enabled = true
require_auth = true
```

## Skills

RustyClaw loads skills from:

1. `<workspace>/skills` (highest precedence)
2. `~/.rustyclaw/skills`
3. Bundled skills (lowest precedence)

### Skill Format (SKILL.md)

```markdown
---
name: my-skill
description: Does something useful
metadata: {"openclaw": {"requires": {"bins": ["git"]}}}
---

# Instructions

Use git to do things.
```

### Gating

Skills can require:
- **bins**: Binaries on PATH
- **anyBins**: At least one of these binaries
- **env**: Environment variables
- **config**: Config values
- **os**: Operating systems (darwin, linux, win32)

## Security

- **Encrypted Secrets**: AES-256 encrypted vault
- **TOTP 2FA**: Optional two-factor authentication
- **Access Policies**: Always, WithAuth, SkillOnly, Never
- **Rate Limiting**: Protection against brute force
- **Lockout**: Account lockout after failed attempts

## Testing

```bash
# Run all tests (unit + integration)
cargo test

# Run specific test suite
cargo test --test cli_conformance
cargo test --test gateway_protocol
cargo test --test tool_execution

# Update golden files
UPDATE_GOLDEN=1 cargo test --test golden_files
```

### Test Coverage

- **152+ unit tests** in source modules
- **200+ integration tests** in 7 test files:
  - `cli_conformance.rs` - CLI help and behavior
  - `gateway_protocol.rs` - WebSocket protocol
  - `skill_execution.rs` - Skill loading and gating
  - `tool_execution.rs` - All 30 tools
  - `exit_codes.rs` - Exit code conformance
  - `golden_files.rs` - Help output stability
  - `streaming.rs` - SSE parsing

## Architecture

```
rustyclaw
├── src/
│   ├── tools.rs        # 30 tool definitions
│   ├── gateway.rs      # WebSocket server
│   ├── app.rs          # TUI application
│   ├── skills.rs       # Skill loading + gating
│   ├── messenger.rs    # Messaging backends
│   ├── streaming.rs    # SSE streaming
│   ├── secrets.rs      # Encrypted vault
│   ├── sessions.rs     # Multi-session support
│   ├── memory.rs       # BM25 memory search
│   ├── cron.rs         # Scheduled jobs
│   └── process_manager.rs # Background processes
└── tests/
    └── *.rs            # Integration tests
```

## License

MIT License - See LICENSE file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## See Also

- [OpenClaw](https://github.com/openclaw/openclaw) - The original project
- [PARITY_PLAN.md](PARITY_PLAN.md) - Detailed feature parity tracking
