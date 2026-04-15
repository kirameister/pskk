# PSKK

A modern Japanese Input Method Engine (IME) with support for multiple platforms.

このプロジェクトは、IBus-PSKK を拡張して異なるプラットフォームに対応した実装を行うことを目的にしています。

## Features

- 🎯 **Kana-Kanji Conversion** - Intelligent conversion with dictionary and CRF-based bunsetsu segmentation
- ⚡ **Simultaneous Input** - Multi-key chord input support
- 🔤 **Kanchoku (Direct Kanji Input)** - Two-stroke kanji input mode
- 🎨 **Customizable Layouts** - Configurable input and kanchoku layouts
- 🖥️ **Cross-Platform** - IBus (Linux), with planned support for Windows and macOS
- ⚙️ **Settings App** - GUI configuration tool built with Tauri
- 🧪 **IME Tester** - Platform-independent testing interface

## Installation

### IBus (Linux)

#### Prerequisites

- Rust toolchain (1.70+)
- IBus
- Node.js and npm (for GUI apps)
- `just` command runner (optional but recommended)

```bash
# Install just (optional)
cargo install just

# Install IBus (if not already installed)
# Debian/Ubuntu:
sudo apt install ibus

# Fedora:
sudo dnf install ibus

# Arch:
sudo pacman -S ibus
```

#### Build and Install

```bash
# Clone the repository
git clone https://github.com/yourusername/pskk.git
cd pskk

# Install dependencies
just install-deps

# Build everything
just build-all

# Install to /opt/pskk (requires sudo)
sudo just install-system

# Or use the install script
sudo ./packaging/install.sh
```

#### Configure IBus

After installation, configure IBus to use PSKK:

```bash
# Restart IBus
ibus restart

# Open IBus preferences
ibus-setup
```

In the IBus Preferences window:
1. Go to **Input Method** tab
2. Click **Add**
3. Select **Japanese** → **PSKK**
4. Click **Add**

#### Usage

- **Switch to PSKK**: Use your IBus hotkey (default: `Super+Space` or `Ctrl+Space`)
- **Mode switching**: 
  - `Ctrl+J` or `Ctrl+\` to toggle between Alphanumeric (A) and Hiragana (あ) modes
- **Conversion**:
  - Type romaji in Hiragana mode (e.g., `ai` → `あい`)
  - Press `Space` to convert to kanji
  - Use `↑`/`↓` arrows to select candidates
  - Press `Enter` to confirm
  - Press `Escape` to cancel

### Settings App

Configure PSKK using the GUI settings app:

```bash
# Run settings app
pskk-settings

# Or if not in PATH:
/opt/pskk/bin/pskk-settings
```

### IME Tester (Development Tool)

Test IME behavior without IBus:

```bash
# Run IME tester
just dev-ime-tester

# Or directly:
cd apps/ime-tester
cargo tauri dev
```

## Uninstallation

```bash
# Using just
sudo just uninstall-system

# Or using the uninstall script
sudo ./packaging/uninstall.sh

# Remove user configuration (optional)
rm -rf ~/.config/pskk
```

## Development

### Project Structure

```
pskk/
├── src/                        # Core Rust library
│   ├── engine.rs              # IME state machine
│   ├── henkan.rs              # Kana-kanji conversion
│   ├── kanchoku.rs            # Direct kanji input
│   ├── simultaneous_processor.rs
│   ├── katsuyou.rs            # Verb conjugation
│   ├── settings.rs            # Configuration management
│   └── util.rs                # Utilities
├── apps/
│   ├── settings/              # Settings GUI (Tauri)
│   └── ime-tester/            # IME testing app (Tauri)
├── packaging/                 # Installation scripts
├── data/                      # Dictionaries and models
└── justfile                   # Build commands
```

### Building

```bash
# Build core library
just build

# Build settings app
just settings-tauri-build

# Build IME tester
just ime-tester-build

# Build everything
just build-all

# Run tests
just test

# Check everything
just check-all
```

### Development Workflow

```bash
# Run IME tester for quick testing
just dev-ime-tester

# Run settings app
just dev-settings

# Run tests
just test
```

### Installation Paths

Default installation to `/opt/pskk`:

```
/opt/pskk/
├── bin/
│   ├── pskk-settings          # Settings GUI
│   └── pskk-ime-tester        # IME tester
├── libexec/
│   └── ibus-engine-pskk       # IBus engine
├── lib/
│   └── libpskk.rlib           # Core library
└── share/
    ├── data/                  # Dictionaries, models
    └── ibus/component/        # IBus component definition

/usr/local/bin/
├── pskk-settings -> /opt/pskk/bin/pskk-settings
└── pskk-ime-tester -> /opt/pskk/bin/pskk-ime-tester

~/.config/pskk/
├── config.json                # User configuration
├── user_dictionary.json       # User dictionary
└── layouts/                   # Custom layouts
```

### Custom Installation Path

To install to a different location:

```bash
# Install to custom prefix
sudo just install-system PREFIX=/usr/local/pskk

# Or set environment variable
export PSKK_PREFIX=/usr/local/pskk
sudo ./packaging/install.sh
```

## Documentation

- [Engine Adapter Interface](ENGINE_ADAPTER_INTERFACE.md) - Core IME engine API
- [Deployment Guide](DEPLOYMENT.md) - Detailed deployment instructions
- [Rust Implementation Status](RUST_IMPLEMENTATION_STATUS.md) - Porting progress

## License

[Add your license here]

## Contributing

[Add contribution guidelines here]

## Acknowledgments

Based on IBus-PSKK, extending it for cross-platform support.
