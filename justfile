set shell := ["bash", "-cu"]

default:
  @just --list

# Core library
test:
  cargo test

build:
  cargo build --release

# Internal: Refuse to run install/uninstall recipes as root. Every privileged step already calls
# sudo on its own, so wrapping the whole recipe in sudo only breaks things: user-session commands
# such as `ibus restart` fail (root has no XDG_RUNTIME_DIR / session D-Bus), and root-owned build
# artifacts are left behind in the source tree.
_assert-not-root:
  @if [ "$(id -u)" -eq 0 ]; then \
    echo "✗ Do not run this recipe as root or with sudo." >&2; \
    echo "  Privileged steps already use sudo internally. Running as root would:" >&2; \
    echo "    - break user-session steps such as 'ibus restart' (no session D-Bus)" >&2; \
    echo "    - leave root-owned files in the source tree and under /opt/pskk" >&2; \
    echo "  Re-run it without sudo, e.g.:  just ibus-install" >&2; \
    exit 1; \
  fi

# ============================================================================
# Core Installation (IMF-agnostic)
# ============================================================================

# Install core PSKK components (server, gRPC stubs, data, GUI apps)
core-install:
  @just _assert-not-root
  @echo "=== Installing PSKK Core Components ==="
  just _install-grpc-stubs
  just _ensure-skk-dictionaries
  just _install-server
  just _install-data
  just settings-install
  just dict-editor-install
  just ime-tester-install
  @echo "✓ Core installation complete"

# Internal: Make sure the checked-in Python gRPC stubs can be imported by the installed protobuf
# runtime. protobuf >= 4.x refuses gencode produced by protoc < 3.20 ("Descriptors cannot be created
# directly"), so a stale pskk_pb2.py breaks the IBus engine at import time. Old-style gencode is
# recognised by its direct _descriptor.EnumValueDescriptor()/FileDescriptor() construction.
# File mtimes are deliberately NOT the trigger: a fresh git checkout stamps every file with almost
# the same timestamp, which made the old mtime check silently skip regeneration and install stubs
# that no modern protobuf runtime can import.
_install-grpc-stubs:
  @regen=0; \
  if [ ! -f proto/pskk_pb2.py ] || grep -q "_descriptor.EnumValueDescriptor(" proto/pskk_pb2.py; then \
    echo "  gRPC stubs are missing or use pre-3.20 protobuf gencode - regenerating"; \
    regen=1; \
  elif [ proto/pskk.proto -nt proto/pskk_pb2.py ] && command -v protoc >/dev/null 2>&1; then \
    echo "  proto/pskk.proto is newer than the generated stubs - regenerating"; \
    regen=1; \
  else \
    echo "  ✓ gRPC stubs are up to date"; \
  fi; \
  if [ "$regen" -eq 1 ]; then just _regen-grpc-stubs; fi

# Internal: Regenerate the Python gRPC stubs. Prefers grpcio-tools (regenerates both files); falls
# back to the system protoc for pskk_pb2.py only, because pskk_pb2_grpc.py does not depend on the
# protobuf gencode version and can stay as checked in.
_regen-grpc-stubs:
  @if python3 -c "import grpc_tools" >/dev/null 2>&1; then \
    echo "  Generating gRPC stubs with grpcio-tools..."; \
    python3 -m grpc_tools.protoc -I./proto --python_out=./proto --grpc_python_out=./proto ./proto/pskk.proto; \
  elif command -v protoc >/dev/null 2>&1; then \
    echo "  grpcio-tools not found - regenerating pskk_pb2.py with the system protoc..."; \
    echo "    note: pskk_pb2_grpc.py is only rewritten by grpcio-tools; run ./generate_python_grpc.sh"; \
    echo "          after adding or removing RPCs so the stub picks up the new methods."; \
    protoc -I./proto --python_out=./proto ./proto/pskk.proto; \
  else \
    echo "  ✗ Cannot regenerate the gRPC stubs: neither grpcio-tools nor protoc is installed." >&2; \
    echo "    Install one of the following and retry:" >&2; \
    echo "      sudo apt install python3-grpcio-tools   # Debian/Ubuntu (regenerates both files)" >&2; \
    echo "      sudo apt install protobuf-compiler      # Debian/Ubuntu (pskk_pb2.py only)" >&2; \
    echo "      pip3 install --user grpcio-tools" >&2; \
    exit 1; \
  fi; \
  if grep -q "_descriptor.EnumValueDescriptor(" proto/pskk_pb2.py; then \
    echo "  ✗ Generated pskk_pb2.py still uses pre-3.20 gencode - is your protoc older than 3.20?" >&2; \
    exit 1; \
  else \
    echo "  ✓ gRPC stubs generated"; \
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
  @just _assert-not-root
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
  @if python3 -c "import sys; sys.path.insert(0, '/opt/pskk/libexec'); import pskk_pb2, pskk_pb2_grpc" >/dev/null 2>&1; then \
    echo "  ✓ Installed gRPC stubs import correctly"; \
  else \
    echo "  ✗ The installed gRPC stubs cannot be imported by this Python/protobuf runtime:" >&2; \
    python3 -c "import sys; sys.path.insert(0, '/opt/pskk/libexec'); import pskk_pb2" || true; \
    echo "    Regenerate and reinstall them:  just _regen-grpc-stubs && just _install-ibus-engine" >&2; \
    exit 1; \
  fi
  @echo "  ✓ IBus engine installed"

# Internal: Install IBus component XML
_install-ibus-component:
  @echo "  Registering IBus component..."
  sudo cp packaging/pskk.xml /usr/share/ibus/component/
  @echo "  ✓ IBus component registered"

# Internal: Restart the IBus daemon. This is a *user session* command - it needs the desktop
# session D-Bus, so it cannot work when run as root or from a shell without a session bus.
_restart-ibus:
  @echo "  Restarting IBus..."
  @if [ -n "${DBUS_SESSION_BUS_ADDRESS:-}" ] || { [ -n "${XDG_RUNTIME_DIR:-}" ] && [ -S "${XDG_RUNTIME_DIR}/bus" ]; }; then \
    if ibus restart; then \
      echo "  ✓ IBus restarted"; \
    else \
      echo "  ⚠ IBus not running - start it manually with 'ibus-daemon -drx'"; \
    fi; \
  else \
    echo "  ⚠ No user session D-Bus found (XDG_RUNTIME_DIR unset or has no bus socket)."; \
    echo "    IBus was not restarted. Run 'ibus restart' from your desktop session,"; \
    echo "    or log out and back in, to pick up the new engine."; \
  fi

# Uninstall PSKK from IBus (removes IBus integration + core components)
ibus-uninstall:
  @just _assert-not-root
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
# Fcitx 5-Specific Installation
# ============================================================================

# Install PSKK for Fcitx 5 (includes core + Fcitx 5 addon)
fcitx5-install:
  @just _assert-not-root
  @echo "=== Installing PSKK for Fcitx 5 ==="
  just core-install
  just fcitx5-build
  @sudo cmake --install fcitx5/build
  just _restart-fcitx5
  @echo ""
  @echo "✓ Fcitx 5 installation complete!"
  @echo ""
  @echo "Next steps:"
  @echo "  1. Open fcitx5-configtool → Add Input Method"
  @echo "  2. Search for PSKK and add it"
  @echo "  3. Switch to PSKK and type (Ctrl+J / Ctrl+\ toggles あ/A)"

# Build the Fcitx 5 addon (requires cmake and the fcitx5 dev packages)
fcitx5-build:
  @echo "Building Fcitx 5 addon (requires cmake and the fcitx5 dev packages)..."
  @MULTIARCH="$(dpkg-architecture -qDEB_HOST_MULTIARCH 2>/dev/null || true)"; \
    if [ -n "$MULTIARCH" ]; then CMAKE_LIBDIR="lib/$MULTIARCH"; \
    elif [ -d /usr/lib64 ]; then CMAKE_LIBDIR="lib64"; \
    else CMAKE_LIBDIR="lib"; fi; \
    cmake -S fcitx5 -B fcitx5/build -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=/usr -DCMAKE_INSTALL_LIBDIR="$CMAKE_LIBDIR" && \
    cmake --build fcitx5/build -j"$(nproc)"

# Restart Fcitx 5 (used after install/uninstall)
_restart-fcitx5:
  @fcitx5-remote -r >/dev/null 2>&1 && echo "  ✓ Fcitx 5 restarted" || echo "  ⚠ Fcitx 5 not running - start it manually with 'fcitx5 -d'"

# Uninstall PSKK from Fcitx 5 (removes Fcitx 5 integration + core components)
fcitx5-uninstall:
  @just _assert-not-root
  @echo "=== Uninstalling PSKK from Fcitx 5 ==="
  @sudo rm -f /usr/share/fcitx5/addon/pskk.conf
  @sudo rm -f /usr/share/fcitx5/inputmethod/pskk.conf
  @sudo rm -f /usr/lib/x86_64-linux-gnu/fcitx5/pskk.so /usr/lib/x86_64-linux-gnu/fcitx5/libpskk.so
  @sudo rm -f /usr/lib/aarch64-linux-gnu/fcitx5/pskk.so /usr/lib/aarch64-linux-gnu/fcitx5/libpskk.so
  @sudo rm -f /usr/lib/fcitx5/pskk.so /usr/lib/fcitx5/libpskk.so
  @sudo rm -f /usr/lib64/fcitx5/pskk.so /usr/lib64/fcitx5/libpskk.so
  just _restart-fcitx5
  just core-uninstall
  @echo ""
  @echo "✓ Fcitx 5 uninstallation complete!"
  @echo ""
  @echo "Note: User configuration in ~/.config/pskk was preserved."
  @echo "To remove user data, run: rm -rf ~/.config/pskk"

# ============================================================================
# Core Uninstallation
# ============================================================================

# Uninstall core PSKK components (server, apps, data)
core-uninstall:
  @just _assert-not-root
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

# Build and install the settings GUI app (UI dist is embedded at compile time)
settings-install:
  just settings-ui-install
  just settings-ui-build
  cd apps/settings/src-tauri && cargo build --release --features custom-protocol
  sudo mkdir -p /opt/pskk/bin
  sudo cp apps/settings/src-tauri/target/release/pskk-settings /opt/pskk/bin/
  sudo ln -sf /opt/pskk/bin/pskk-settings /usr/local/bin/pskk-settings
  @echo "✓ Settings app installed to /opt/pskk/bin/pskk-settings"

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

ime-tester-install:
  just ime-tester-ui-install
  just ime-tester-ui-build
  cd apps/ime-tester/src-tauri && cargo build --release --features custom-protocol
  sudo mkdir -p /opt/pskk/bin
  sudo cp apps/ime-tester/src-tauri/target/release/pskk-ime-tester /opt/pskk/bin/
  sudo ln -sf /opt/pskk/bin/pskk-ime-tester /usr/local/bin/pskk-ime-tester
  @echo "✓ IME tester installed to /opt/pskk/bin/pskk-ime-tester"

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
  just dict-editor-ui-install
  just dict-editor-ui-build
  cd apps/dictionary-editor/src-tauri && cargo build --release --features custom-protocol
  sudo mkdir -p /opt/pskk/bin
  sudo cp apps/dictionary-editor/src-tauri/target/release/pskk-dictionary-editor /opt/pskk/bin/
  sudo ln -sf /opt/pskk/bin/pskk-dictionary-editor /usr/local/bin/pskk-dictionary-editor
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
