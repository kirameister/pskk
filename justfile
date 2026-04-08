set shell := ["zsh", "-cu"]

default:
  @just --list

test:
  cargo test

build:
  cargo build

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
