set shell := ["zsh", "-cu"]

default:
  @just --list

# Core library
test:
  cargo test

build:
  cargo build --release

# gRPC Server
server-build:
  cargo build --release --bin pskk-server

server-run:
  cargo run --bin pskk-server

server-dev:
  cargo run --bin pskk-server

server-install:
  cargo build --release --bin pskk-server
  sudo mkdir -p /opt/pskk/bin
  sudo cp target/release/pskk-server /opt/pskk/bin/
  @echo "✓ Server installed to /opt/pskk/bin/pskk-server"

# IBus Engine (Python client)
ibus-generate-grpc:
  python3 -m grpc_tools.protoc -I./proto --python_out=./proto --grpc_python_out=./proto ./proto/pskk.proto

ibus-run:
  ./ibus-engine-pskk.py

ibus-install:
  sudo mkdir -p /opt/pskk/libexec
  sudo cp ibus-engine-pskk.py /opt/pskk/libexec/
  sudo chmod +x /opt/pskk/libexec/ibus-engine-pskk.py
  sudo cp proto/pskk_pb2.py proto/pskk_pb2_grpc.py /opt/pskk/libexec/ || echo "Run 'just ibus-generate-grpc' first"
  sudo cp packaging/pskk.xml /usr/share/ibus/component/
  ibus restart

# Install everything (server + client)
install:
  just ibus-generate-grpc
  just server-install
  just ibus-install
  @echo "✓ PSKK installed successfully!"

# Settings app
settings-ui-install:
  cd apps/settings/ui && npm install

settings-ui-build:
  cd apps/settings/ui && npm run build

settings-tauri-build:
  cd apps/settings/src-tauri && cargo tauri build

settings-dev:
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
  cd apps/ime-tester && cargo tauri dev

ime-tester-build:
  cd apps/ime-tester && cargo tauri build

# Dictionary editor app
dict-editor-ui-install:
  cd apps/dictionary-editor/ui && npm install

dict-editor-ui-build:
  cd apps/dictionary-editor/ui && npm run build

dict-editor-dev:
  cd apps/dictionary-editor/src-tauri && cargo tauri dev

dict-editor-build:
  cd apps/dictionary-editor/src-tauri && cargo tauri build

# Install all dependencies
install-deps:
  just settings-ui-install
  just ime-tester-ui-install
  just dict-editor-ui-install

# Build everything
build-all:
  cargo build --release
  just settings-ui-build
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
  cd apps/settings/src-tauri && cargo tauri build
  @echo "Packages created in apps/settings/src-tauri/target/release/bundle/"

package-ime-tester:
  cd apps/ime-tester/src-tauri && cargo tauri build
  @echo "Packages created in apps/ime-tester/src-tauri/target/release/bundle/"

package-dict-editor:
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
