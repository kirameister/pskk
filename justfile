set shell := ["zsh", "-cu"]

default:
  @just --list

# Core library
test:
  cargo test

build:
  cargo build --release

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
  cd apps/ime-tester/src-tauri && cargo tauri build

# Install all dependencies
install-deps:
  just settings-ui-install
  just ime-tester-ui-install

# Build everything
build-all:
  cargo build --release
  just settings-ui-build
  just settings-tauri-build
  just ime-tester-ui-build
  just ime-tester-build

# Development workflow
dev-settings:
  just settings-dev

dev-ime-tester:
  just ime-tester-dev

# Check everything works
check-all:
  cargo test
  cargo clippy -- -D warnings
  just settings-ui-build
  just ime-tester-ui-build

# Clean build artifacts
clean:
  cargo clean
  rm -rf apps/settings/ui/node_modules apps/settings/ui/dist
  rm -rf apps/ime-tester/ui/node_modules apps/ime-tester/ui/dist
  rm -rf apps/settings/src-tauri/target
  rm -rf apps/ime-tester/src-tauri/target

# Package for distribution
package-settings:
  cd apps/settings/src-tauri && cargo tauri build
  @echo "Packages created in apps/settings/src-tauri/target/release/bundle/"

package-ime-tester:
  cd apps/ime-tester/src-tauri && cargo tauri build
  @echo "Packages created in apps/ime-tester/src-tauri/target/release/bundle/"

package-all:
  just package-settings
  just package-ime-tester

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

# Install locally (for testing)
install-local:
  mkdir -p ~/.local/bin
  cp apps/settings/src-tauri/target/release/pskk-settings ~/.local/bin/ 2>/dev/null || echo "Build settings first with: just settings-tauri-build"
  cp apps/ime-tester/src-tauri/target/release/pskk-ime-tester ~/.local/bin/ 2>/dev/null || echo "Build ime-tester first with: just ime-tester-build"
  @echo "Installed to ~/.local/bin (ensure it's in your PATH)"

# Uninstall local installation
uninstall-local:
  rm -f ~/.local/bin/pskk-settings
  rm -f ~/.local/bin/pskk-ime-tester
  @echo "Removed from ~/.local/bin"

# Install to /opt/pskk (system-wide, requires sudo)
install-system PREFIX="/opt/pskk":
  @echo "Installing to {{PREFIX}}..."
  sudo mkdir -p {{PREFIX}}/bin
  sudo mkdir -p {{PREFIX}}/lib
  sudo mkdir -p {{PREFIX}}/data
  sudo cp target/release/libpskk.rlib {{PREFIX}}/lib/ 2>/dev/null || echo "Core library not built"
  sudo cp apps/settings/src-tauri/target/release/pskk-settings {{PREFIX}}/bin/ 2>/dev/null || echo "Settings app not built"
  sudo cp apps/ime-tester/src-tauri/target/release/pskk-ime-tester {{PREFIX}}/bin/ 2>/dev/null || echo "IME tester not built"
  sudo mkdir -p /usr/local/bin
  sudo ln -sf {{PREFIX}}/bin/pskk-settings /usr/local/bin/pskk-settings
  sudo ln -sf {{PREFIX}}/bin/pskk-ime-tester /usr/local/bin/pskk-ime-tester
  @echo "Installed to {{PREFIX}}"
  @echo "Symlinks created in /usr/local/bin"

# Uninstall from /opt/pskk
uninstall-system PREFIX="/opt/pskk":
  @echo "Uninstalling from {{PREFIX}}..."
  sudo rm -f /usr/local/bin/pskk-settings
  sudo rm -f /usr/local/bin/pskk-ime-tester
  sudo rm -rf {{PREFIX}}
  @echo "Uninstalled from {{PREFIX}}"

# Install IBus engine (when implemented)
install-ibus PREFIX="/opt/pskk":
  @echo "Installing IBus engine to {{PREFIX}}..."
  sudo mkdir -p {{PREFIX}}/libexec
  sudo mkdir -p {{PREFIX}}/share/ibus/component
  sudo cp target/release/ibus-engine-pskk {{PREFIX}}/libexec/ 2>/dev/null || echo "IBus engine not built yet"
  sudo chmod +x {{PREFIX}}/libexec/ibus-engine-pskk 2>/dev/null || true
  @echo "TODO: Copy IBus component XML to {{PREFIX}}/share/ibus/component/"
  @echo "TODO: Register with IBus"
  @echo "Run 'ibus restart' after installation"

# Uninstall IBus engine
uninstall-ibus PREFIX="/opt/pskk":
  sudo rm -f {{PREFIX}}/libexec/ibus-engine-pskk
  sudo rm -f {{PREFIX}}/share/ibus/component/pskk.xml
  @echo "Run 'ibus restart' to complete uninstallation"
