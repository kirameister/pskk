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
