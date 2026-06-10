# PSKK IBus Setup Guide

## Architecture

PSKK uses a client-server architecture:

```
IBus ↔ Python Client (ibus-engine-pskk.py) ↔ gRPC ↔ Rust Server (pskk-server)
```

- **Python Client**: Thin wrapper that implements the IBus.Engine interface
  - **Auto-starts the server** if not already running
  - Manages server lifecycle
- **Rust Server**: Core engine logic, dictionary handling, conversion
  - Runs in the background
  - Shared across all applications

## Prerequisites

### System Packages

```bash
# Ubuntu/Debian
sudo apt install python3 python3-gi python3-grpc-tools ibus

# Fedora
sudo dnf install python3 python3-gobject python3-grpcio-tools ibus

# Arch
sudo pacman -S python python-gobject python-grpcio-tools ibus
```

### Python Packages

If your system doesn't have `python3-grpc-tools`:

```bash
pip3 install --user grpcio-tools
```

## Setup Steps

### 1. Generate Python gRPC Stubs

```bash
just ibus-generate-grpc
```

This generates:
- `proto/pskk_pb2.py` - Protocol buffer messages
- `proto/pskk_pb2_grpc.py` - gRPC client stub

### 2. Build the Rust Server

```bash
just server-build
```

### 3. Test Locally

The server will auto-start, so you only need to run:

```bash
just ibus-run
```

Or run directly:
```bash
./ibus-engine-pskk.py
```

The Python client will automatically start `pskk-server` if it's not already running.

### 4. Install System-Wide

```bash
# Build everything first
just server-build
just ibus-generate-grpc

# Install (requires sudo)
just ibus-install

# Or use the full install script
sudo ./packaging/install.sh
```

### 5. Configure IBus

```bash
# Restart IBus
ibus restart

# Open IBus preferences
ibus-setup
```

In IBus Preferences:
1. Go to **Input Method** tab
2. Click **Add**
3. Select **Japanese** → **PSKK**
4. Click **Add**

### 6. Start Using

- Switch to PSKK using your input method hotkey (usually `Super+Space`)
- You should see "あ" in the menu bar
- Start typing!

## Troubleshooting

### "No module named grpc_tools"

Install grpcio-tools:
```bash
pip3 install --user grpcio-tools
```

### "Failed to connect to PSKK gRPC server"

The client should auto-start the server, but if it fails:

1. Check if the server binary exists:
```bash
ls -l /opt/pskk/bin/pskk-server
# or for development:
ls -l target/release/pskk-server
```

2. Try starting it manually to see errors:
```bash
/opt/pskk/bin/pskk-server
# or
cargo run --bin pskk-server
```

3. Check if port 50051 is already in use:
```bash
lsof -i :50051
```

### "Failed to connect to IBus daemon"

Restart IBus:
```bash
ibus restart
```

### Check Logs

IBus engine logs:
```bash
journalctl -f | grep pskk
```

Or run the engine directly to see output:
```bash
./ibus-engine-pskk.py
```

## Development Workflow

1. **Make changes to Rust engine** (`src/engine.rs`, etc.)
2. **Rebuild server**: `cargo build --bin pskk-server`
3. **Restart**: 
   - Kill any running `pskk-server` process: `pkill pskk-server`
   - The Python client will auto-start the new version next time you use it
   - Or restart IBus: `ibus restart`

No need to restart the Python client unless you modify `ibus-engine-pskk.py`.

**Quick development cycle:**
```bash
# Make changes to Rust code
cargo build --bin pskk-server
pkill pskk-server
# Switch to PSKK in any app - new server auto-starts
```

## File Locations

After installation:

- **Python client**: `/opt/pskk/libexec/ibus-engine-pskk.py`
- **gRPC stubs**: `/opt/pskk/libexec/pskk_pb2*.py`
- **Rust server**: `/opt/pskk/bin/pskk-server`
- **IBus component**: `/usr/share/ibus/component/pskk.xml`
- **Settings app**: `/opt/pskk/bin/pskk-settings`
- **Data files**: `/opt/pskk/share/`

## Next Steps

- Configure PSKK settings: `pskk-settings`
- Customize layouts in `~/.config/pskk/config.json`
- Add user dictionary entries in `~/.config/pskk/user_dictionary.json`
