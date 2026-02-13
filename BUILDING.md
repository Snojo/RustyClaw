# Building RustyClaw

## Quick Install (crates.io)

```bash
cargo install rustyclaw
```

This installs RustyClaw with default features (no messengers).

## Feature Flags

| Feature | Description | crates.io |
|---------|-------------|-----------|
| `matrix` | Matrix messenger support | ✅ |
| `browser` | CDP browser automation | ✅ |
| `full` | matrix + browser | ✅ |
| `signal` | Signal messenger | ❌ (source only) |

### Install with Features

```bash
# With Matrix support
cargo install rustyclaw --features matrix

# With browser automation
cargo install rustyclaw --features browser

# Full (Matrix + Browser)
cargo install rustyclaw --features full
```

## Building from Source

### Basic Build

```bash
git clone https://github.com/rexlunae/RustyClaw.git
cd RustyClaw
cargo build --release
```

### Release Build (Optimized)

```bash
cargo build --release
```

Binary at `target/release/rustyclaw` (~11 MB with LTO).

## Signal Messenger Support

> ⚠️ **Signal requires building from source.** The presage library is not
> available on crates.io in a compatible version.

### Prerequisites

Signal support uses [presage](https://github.com/whisperfish/presage), which
depends on Signal's cryptographic libraries. These aren't published to crates.io,
so Signal support requires building from the git repository.

### Building with Signal

1. **Clone the repository:**
   ```bash
   git clone https://github.com/rexlunae/RustyClaw.git
   cd RustyClaw
   ```

2. **Enable Signal in Cargo.toml:**
   
   Uncomment the signal dependencies:
   ```toml
   [features]
   signal = ["dep:presage", "dep:presage-store-sqlite"]
   
   [dependencies]
   presage = { git = "https://github.com/whisperfish/presage", optional = true }
   presage-store-sqlite = { git = "https://github.com/whisperfish/presage", optional = true }
   
   [patch.crates-io]
   presage = { git = "https://github.com/whisperfish/presage" }
   presage-store-sqlite = { git = "https://github.com/whisperfish/presage" }
   curve25519-dalek = { git = "https://github.com/signalapp/curve25519-dalek", tag = "signal-curve25519-4.1.3" }
   ```

3. **Build with Signal feature:**
   ```bash
   cargo build --release --features signal
   ```

4. **Link your Signal account:**
   ```bash
   rustyclaw signal link
   ```
   This generates a QR code to scan with your Signal app.

### Why Signal Isn't on crates.io

The Signal integration depends on:
- `presage` — Signal protocol client (git-only, 0.8.0-dev)
- `presage-store-sqlite` — Storage backend (not on crates.io)
- `libsignal-service` — Protocol implementation (git-only)
- `libsignal-*` — Signal's crypto libraries (git-only)

These libraries are maintained separately and not regularly published to crates.io.
We'll add crates.io support when upstream publishes compatible versions.

## Cross-Compilation

### Linux (from macOS)

```bash
# Install target
rustup target add x86_64-unknown-linux-gnu

# Build
cargo build --release --target x86_64-unknown-linux-gnu
```

### macOS (from Linux)

Requires osxcross toolchain. See [cross-rs](https://github.com/cross-rs/cross).

## Minimum Supported Rust Version

Rust 1.85 or later (Edition 2024).

## Troubleshooting

### Signal build fails with crypto errors

Ensure the curve25519-dalek patch is in your Cargo.toml:
```toml
[patch.crates-io]
curve25519-dalek = { git = "https://github.com/signalapp/curve25519-dalek", tag = "signal-curve25519-4.1.3" }
```

### SQLite linking errors (Signal)

Install SQLite development headers:
```bash
# Ubuntu/Debian
sudo apt install libsqlite3-dev

# macOS
brew install sqlite
```

### Browser feature needs chromium

The `browser` feature uses CDP (Chrome DevTools Protocol). Install Chrome/Chromium:
```bash
# Ubuntu/Debian
sudo apt install chromium-browser

# macOS
brew install --cask chromium
```
