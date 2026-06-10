# PSKK IBus Setup Guide

## Architecture

PSKK uses a client-server architecture:

```
IBus ↔ Python Client (ibus-engine-pskk.py) ↔ gRPC ↔ Rust Server (pskk-server)
```

- **Python Client**: Thin wrapper that implements the IBus.Engine interface
- **Rust Server**: Core engine logic, dictionary handling, conversion

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

Terminal 1 - Start the gRPC server:
```bash
just server-run
```

Terminal 2 - Run the IBus engine:
```bash
just ibus-run
```

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

Make sure the server is running:
```bash
cargo run --bin pskk-server
```

Or check if it's installed:
```bash
/opt/pskk/bin/pskk-server
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
3. **Restart server**: Kill and restart `pskk-server`
4. **Test**: The Python client will automatically use the new server

No need to restart the Python client unless you modify `ibus-engine-pskk.py`.

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
