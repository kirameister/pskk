set shell := ["bash", "-cu"]

default:
  @just --list

# Core library
test:
  cargo test

build:
  cargo build --release

# ============================================================================
# Core Installation (IMF-agnostic)
# ============================================================================

# Install core PSKK components (server, gRPC stubs, data)
core-install:
  @echo "=== Installing PSKK Core Components ==="
  just _install-grpc-stubs
  just _ensure-skk-dictionaries
  just _install-server
  just _install-data
  @echo "✓ Core installation complete"

# Internal: Generate and install gRPC stubs (only when the .proto changed — avoids requiring grpcio-tools for plain installs)
_install-grpc-stubs:
  @if [ proto/pskk.proto -nt proto/pskk_pb2.py ] || [ proto/pskk.proto -nt proto/pskk_pb2_grpc.py ]; then \
    echo "  Generating gRPC stubs..."; \
    python3 -m grpc_tools.protoc -I./proto --python_out=./proto --grpc_python_out=./proto ./proto/pskk.proto; \
    echo "  ✓ gRPC stubs generated"; \
  else \
    echo "  ✓ gRPC stubs are up to date (skipping regeneration)"; \
  fi

# Internal: Build and install gRPC server
_install-server:
  @echo "  Building and installing pskk-server..."
  cargo build --release --bin pskk-server
  @pkill pskk-server || true
  sudo mkdir -p /opt/pskk/bin
  sudo cp target/release/pskk-server /opt/pskk/bin/
  @echo "  ✓ pskk-server installed to /opt/pskk/bin/pskk-server"

# Internal: Install data files (default config, layouts, kanchoku layouts)
_install-data:
  @echo "  Installing data files..."
  sudo mkdir -p /opt/pskk/data
  sudo cp -r data/* /opt/pskk/data/
  sudo chmod -R a+rX /opt/pskk/data
  @echo "  ✓ Data files installed to /opt/pskk/data"

# Internal: Download SKK dictionaries if missing (not shipped in the repo due to license).
# Downloads into data/skk_dict/ so that _install-data copies them to /opt/pskk/data/skk_dict.
_ensure-skk-dictionaries:
  @if [ -f "data/skk_dict/SKK-JISYO.L" ]; then \
    echo "  ✓ SKK dictionaries already present"; \
  elif ! command -v curl >/dev/null 2>&1; then \
    echo "  ⚠ curl not found - cannot download SKK dictionaries (henkan will have no dictionary)"; \
  else \
    echo "  Downloading SKK dictionaries to data/skk_dict/..."; \
    mkdir -p data/skk_dict; \
    for file in SKK-JISYO.L SKK-JISYO.M SKK-JISYO.ML SKK-JISYO.S; do \
      echo "    Downloading $file..."; \
      if curl -f -L -o "data/skk_dict/$file" "https://raw.githubusercontent.com/skk-dev/dict/master/$file"; then \
        iconv -f EUC-JP -t UTF-8 "data/skk_dict/$file" > "data/skk_dict/$file.utf8" && mv "data/skk_dict/$file.utf8" "data/skk_dict/$file" || rm -f "data/skk_dict/$file.utf8"; \
        echo "    ✓ $file downloaded"; \
      else \
        echo "    ⚠ Failed to download $file (skipping)"; \
      fi; \
    done; \
  fi

# ============================================================================
# IBus-Specific Installation
# ============================================================================

# Install PSKK for IBus (includes core + IBus integration)
ibus-install:
  @echo "=== Installing PSKK for IBus ==="
  just core-install
  just _install-ibus-engine
  just _install-ibus-component
  just _restart-ibus
  @echo ""
  @echo "✓ IBus installation complete!"
  @echo ""
  @echo "Next steps:"
  @echo "  1. Open IBus preferences: ibus-setup"
  @echo "  2. Go to Input Method tab and click Add"
  @echo "  3. Select Japanese → PSKK"

# Internal: Install Python dependencies required by the IBus engine (grpc, protobuf, gi) if missing
_ensure-ibus-python-deps:
  @if ! python3 -c "import grpc, google.protobuf, gi" >/dev/null 2>&1; then \
    echo "  Installing IBus engine Python dependencies (python3-grpcio, python3-protobuf, python3-gi)..."; \
    sudo apt-get install -y python3-grpcio python3-protobuf python3-gi; \
  else \
    echo "  ✓ IBus engine Python dependencies present"; \
  fi

# Internal: Install IBus Python engine
_install-ibus-engine:
  just _ensure-ibus-python-deps
  @echo "  Installing IBus engine..."
  sudo mkdir -p /opt/pskk/libexec
  sudo cp ibus-engine-pskk.py /opt/pskk/libexec/
  sudo chmod +x /opt/pskk/libexec/ibus-engine-pskk.py
  sudo cp proto/pskk_pb2.py proto/pskk_pb2_grpc.py /opt/pskk/libexec/
  @echo "  ✓ IBus engine installed"

# Internal: Install IBus component XML
_install-ibus-component:
  @echo "  Registering IBus component..."
  sudo cp packaging/pskk.xml /usr/share/ibus/component/
  @echo "  ✓ IBus component registered"

# Internal: Restart IBus daemon
_restart-ibus:
  @echo "  Restarting IBus..."
  @ibus restart || echo "  ⚠ IBus not running - start it manually with 'ibus-daemon -drx'"

# Uninstall PSKK from IBus (removes IBus integration + core components)
ibus-uninstall:
  @echo "=== Uninstalling PSKK from IBus ==="
  just _uninstall-ibus-component
  just _uninstall-ibus-engine
  just core-uninstall
  just _restart-ibus
  @echo ""
  @echo "✓ IBus uninstallation complete!"
  @echo ""
  @echo "Note: User configuration in ~/.config/pskk was preserved."
  @echo "To remove user data, run: rm -rf ~/.config/pskk"

# Internal: Remove IBus component XML
_uninstall-ibus-component:
  @echo "  Removing IBus component..."
  sudo rm -f /usr/share/ibus/component/pskk.xml
  @echo "  ✓ IBus component removed"

# Internal: Remove IBus Python engine
_uninstall-ibus-engine:
  @echo "  Removing IBus engine..."
  sudo rm -f /opt/pskk/libexec/ibus-engine-pskk.py
  sudo rm -f /opt/pskk/libexec/pskk_pb2.py
  sudo rm -f /opt/pskk/libexec/pskk_pb2_grpc.py
  sudo rmdir /opt/pskk/libexec 2>/dev/null || true
  @echo "  ✓ IBus engine removed"

# ============================================================================
# Core Uninstallation
# ============================================================================

# Uninstall core PSKK components (server, apps, data)
core-uninstall:
  @echo "=== Uninstalling PSKK Core Components ==="
  just _uninstall-server
  @echo "  Removing /opt/pskk directory..."
  sudo rm -rf /opt/pskk
  @echo "  ✓ Core components removed"
  @echo "✓ Core uninstallation complete"

# Internal: Remove gRPC server
_uninstall-server:
  @echo "  Stopping and removing pskk-server..."
  @pkill pskk-server || true
  sudo rm -f /opt/pskk/bin/pskk-server
  sudo rmdir /opt/pskk/bin 2>/dev/null || true
  @echo "  ✓ pskk-server removed"

# ============================================================================
# Development helpers for IBus
# ============================================================================

# Run IBus engine directly (for testing)
ibus-run:
  ./ibus-engine-pskk.py

# Build gRPC server for development
server-build:
  cargo build --release --bin pskk-server

# Run gRPC server directly
server-run:
  cargo run --bin pskk-server

# Run gRPC server in development mode
server-dev:
  cargo run --bin pskk-server

# Internal: Install the Tauri CLI (v2) once, if missing — required by every `cargo tauri` recipe
_ensure-tauri-cli:
  @if ! command -v cargo-tauri >/dev/null 2>&1 || ! cargo tauri --version 2>/dev/null | grep -q 'tauri-cli 2\.'; then echo "Installing tauri-cli..."; cargo install tauri-cli; fi

# Settings app
settings-ui-install:
  cd apps/settings/ui && npm install

settings-ui-build:
  cd apps/settings/ui && npm run build

settings-tauri-build:
  just _ensure-tauri-cli
  cd apps/settings/src-tauri && cargo tauri build

settings-dev:
  just _ensure-tauri-cli
  cd apps/settings/src-tauri && cargo tauri dev

settings-check:
  cargo test
  cd apps/settings/ui && npm run build

# IME tester app
ime-tester-ui-install:
  cd apps/ime-tester/ui && npm install

ime-tester-ui-build:
  cd apps/ime-tester/ui && npm run build

ime-tester-dev:
  just _ensure-tauri-cli
  cd apps/ime-tester && cargo tauri dev

ime-tester-build:
  just _ensure-tauri-cli
  cd apps/ime-tester && cargo tauri build

# Dictionary editor app
dict-editor-ui-install:
  cd apps/dictionary-editor/ui && npm install

dict-editor-ui-build:
  cd apps/dictionary-editor/ui && npm run build

dict-editor-dev:
  just _ensure-tauri-cli
  cd apps/dictionary-editor/src-tauri && cargo tauri dev

dict-editor-build:
  just _ensure-tauri-cli
  cd apps/dictionary-editor/src-tauri && cargo tauri build

dict-editor-install:
  cd apps/dictionary-editor/src-tauri && cargo build --release
  sudo mkdir -p /opt/pskk/bin
  sudo cp apps/dictionary-editor/src-tauri/target/release/pskk-dictionary-editor /opt/pskk/bin/
  @echo "✓ Dictionary editor installed to /opt/pskk/bin/pskk-dictionary-editor"

# Install all dependencies
install-deps:
  just settings-ui-install
  just ime-tester-ui-install
  just dict-editor-ui-install

# Build everything
build-all:
  cargo build --release
  just settings-ui-build
  just _ensure-tauri-cli
  just settings-tauri-build
  just ime-tester-ui-build
  just ime-tester-build
  just dict-editor-ui-build
  just dict-editor-build

# Development workflow
dev-settings:
  just settings-dev

dev-ime-tester:
  just ime-tester-dev

dev-dict-editor:
  just dict-editor-dev

# Check everything works
check-all:
  cargo test
  cargo clippy -- -D warnings
  just settings-ui-build
  just ime-tester-ui-build
  just dict-editor-ui-build

# Clean build artifacts
clean:
  cargo clean
  rm -rf apps/settings/ui/node_modules apps/settings/ui/dist
  rm -rf apps/ime-tester/ui/node_modules apps/ime-tester/ui/dist
  rm -rf apps/dictionary-editor/ui/node_modules apps/dictionary-editor/ui/dist
  rm -rf apps/settings/src-tauri/target
  rm -rf apps/ime-tester/src-tauri/target
  rm -rf apps/dictionary-editor/src-tauri/target

# Package for distribution
package-settings:
  just _ensure-tauri-cli
  cd apps/settings/src-tauri && cargo tauri build
  @echo "Packages created in apps/settings/src-tauri/target/release/bundle/"

package-ime-tester:
  just _ensure-tauri-cli
  cd apps/ime-tester/src-tauri && cargo tauri build
  @echo "Packages created in apps/ime-tester/src-tauri/target/release/bundle/"

package-dict-editor:
  just _ensure-tauri-cli
  cd apps/dictionary-editor/src-tauri && cargo tauri build
  @echo "Packages created in apps/dictionary-editor/src-tauri/target/release/bundle/"

package-all:
  just package-settings
  just package-ime-tester
  just package-dict-editor

# Show build outputs
show-outputs:
  @echo "=== Core Library ==="
  @ls -lh target/release/libpskk.* 2>/dev/null || echo "Not built yet"
  @echo ""
  @echo "=== Settings App ==="
  @ls -lh apps/settings/src-tauri/target/release/pskk-settings 2>/dev/null || echo "Not built yet"
  @echo ""
  @echo "=== Settings Packages ==="
  @find apps/settings/src-tauri/target/release/bundle -name "*.deb" -o -name "*.AppImage" -o -name "*.rpm" 2>/dev/null || echo "Not packaged yet"
  @echo ""
  @echo "=== IME Tester ==="
  @ls -lh apps/ime-tester/src-tauri/target/release/pskk-ime-tester 2>/dev/null || echo "Not built yet"
  @echo ""
  @echo "=== IME Tester Packages ==="
  @find apps/ime-tester/src-tauri/target/release/bundle -name "*.deb" -o -name "*.AppImage" -o -name "*.rpm" 2>/dev/null || echo "Not packaged yet"
  @echo ""
  @echo "=== Dictionary Editor ==="
  @ls -lh apps/dictionary-editor/src-tauri/target/release/pskk-dictionary-editor 2>/dev/null || echo "Not built yet"
  @echo ""
  @echo "=== Dictionary Editor Packages ==="
  @find apps/dictionary-editor/src-tauri/target/release/bundle -name "*.deb" -o -name "*.AppImage" -o -name "*.rpm" 2>/dev/null || echo "Not packaged yet"
