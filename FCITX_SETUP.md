# PSKK Fcitx 5 Setup Guide

This document explains how PSKK runs under **Fcitx 5**. The IBus client is a
Python engine (`ibus-engine-pskk.py`); the Fcitx 5 frontend is a native C++
addon under [`fcitx5/`](fcitx5/). Both are thin adapters around the same
engine core (the Rust `pskk-server`).

## Architecture

```
IBus        ↔ ibus-engine-pskk.py (Python, gRPC)
Fcitx 5     ↔ pskk.so             (C++ addon, JSON/TCP)   ──►  pskk-server (Rust core)
                                    ↕ auto-start / reconnect
```

- `pskk-server` is framework-agnostic and stays the single owner of IME state,
  dictionaries and conversion.
- The IBus client talks to it over gRPC on `127.0.0.1:50051` (unchanged).
- The Fcitx addon talks to it over a lightweight newline-delimited **JSON/TCP**
  protocol on `127.0.0.1:50052` (see below). The addon therefore has **no
  gRPC/protobuf C++ dependency** — only `libfcitx5core`.

Both listeners share one engine instance in `pskk-server`, so running IBus and
Fcitx clients at different times against the same server is fine (only one
framework should be active per desktop session; see [Switching](#switching-a-session-to-fcitx-5)).

## Prerequisites (Ubuntu 24.04 / Debian)

```bash
sudo apt install fcitx5 fcitx5-frontend-gtk3 fcitx5-frontend-gtk4 \
                 fcitx5-frontend-qt5 fcitx5-config-qt cmake \
                 libfcitx5core-dev libfcitx5utils-dev libfcitx5config-dev
# Optionally replace the IBus frontends' GTK/Qt IM modules with Fcitx's
# (see "Switching a session to Fcitx 5" below).
```

Note: the engine headers pull in `<fcitx-utils/...>` and `<fcitx-config/...>`
in addition to `<fcitx/...>`, hence the three `-dev` packages. CMake locates
fcitx5 via pkg-config (`Fcitx5Core`/`Fcitx5Utils`/`Fcitx5Config`) and falls
back to probing `/usr/include/Fcitx5/{Core,Utils,Config}` (the Debian/Ubuntu
layout) or a flat `/usr/include` layout, honoring `-DFCITX5_CORE_LIBRARY=` /
`-DFCITX5_UTILS_LIBRARY=` overrides.

## Building and installing

```bash
# Build the addon into fcitx5/build and install it system-wide (requires sudo)
just fcitx5-build
just fcitx5-install        # = core-install + fcitx5-build + cmake --install + fcitx5 restart

# Or via the generic installer after a manual `just fcitx5-build`:
sudo ./packaging/install.sh
```

`fcitx5-install` writes these artifacts (mirroring how fcitx5-skk installs):

| Artifact                              | Destination                                                          |
|---------------------------------------|---------------------------------------------------------------------|
| addon metadata `pskk.conf`            | `/usr/share/fcitx5/addon/pskk.conf`                                 |
| input method entry `pskk.conf`        | `/usr/share/fcitx5/inputmethod/pskk.conf`                           |
| engine library `pskk.so`              | `/usr/lib/<multiarch>/fcitx5/pskk.so` (Debian), else `/usr/lib/fcitx5/` |
| mode icons `pskk-hiragana`/`pskk-alphanumeric` | `/usr/share/icons/hicolor/<size>x<size>/apps/*.png`          |

> **Naming matters**: fcitx loads an addon library by the exact name
> `Library` + `.so` (see `addonloader.cpp`). `Library=pskk` therefore
> requires the file to be named **`pskk.so`** — not `libpskk.so` (that is why
> fcitx5-skk ships `skk.so`, fcitx5-mozc ships `mozc.so`, etc.). If an input
> method shows "(Not available)", check for this first:

After installing (or updating) the icons, refresh the icon-theme cache once:

```bash
sudo gtk-update-icon-cache -f /usr/share/icons/hicolor
```

Uninstall: `sudo just fcitx5-uninstall` (or `sudo ./packaging/uninstall.sh`).

## Enabling PSKK in Fcitx 5

1. Restart Fcitx: `fcitx5-remote -r` (or start it: `fcitx5 -d`).
2. Run `fcitx5-configtool` → **Add Input Method** → search **PSKK** → add.
3. Optional: `fcitx5-configtool` → *Global Options* → set a trigger key
   (default `Ctrl+Space`) and switch to PSKK.

Usage is identical to IBus: romaji in Hiragana mode, `Space` converts,
`↑`/`↓` select, `Enter` commits, `Esc` cancels, `Ctrl+J`/`Ctrl+\` toggles
`あ`/`A`. The mode switch is exposed in the input-method menu/status area
(`あ` / `A` radio actions, plus **Settings**, **Dictionary Editor** and
**IME Tester**).

### Switching a session to Fcitx 5

IBus and Fcitx 5 must not both own the session's IM modules:

```bash
im-config -n fcitx5    # Debian/Ubuntu helper; then log out and back in
```

or export manually for one app:

```bash
GTK_IM_MODULE=fcitx QT_IM_MODULE=fcitx XMODIFIERS=@im=fcitx some-app
```

## The JSON/TCP protocol

`pskk-server` binds a second listener (`127.0.0.1:50052`, override with
`PSKK_JSON_ADDR`; gRPC stays on `50051`, override `PSKK_GRPC_ADDR`). One JSON
object per line; requests carry an `op` field; responses are the serialized
response message with numeric enum values (`InputMode`: 0 = `A`, 1 = `あ`;
`ResponseStatus`: 0 = OK, 1 = `HENKAN_UNAVAILABLE`); errors are
`{"error": "..."}`.

```text
{"op":"process_key","key_char":"a","key_name":"a","is_pressed":true,
 "modifiers":{"shift":false,"ctrl":false,"alt":false,"super":false}}
{"op":"set_mode","mode":1}
{"op":"get_mode"}          → {"mode":1}
{"op":"focus_out"}         → EngineOutput
{"op":"reset"}             → {}
{"op":"get_dictionary_size"} → {"size":12345}
```

`process_key`/`set_mode`/`focus_out` answer with an `EngineOutput` object:
`commit_string`, `preedit_segments[{text,is_selected}]`, `preedit_cursor_pos`,
`candidates[{surface,reading}]`, `candidate_cursor_pos`, `show_candidates`,
`consumed`, `current_mode`, `marker_state`, `engine_state`, `status`.
Source of truth: [`src/json/server.rs`](src/json/server.rs).

Test it without Fcitx (server running):

```bash
python3 -c "
import json, socket
s = socket.create_connection(('127.0.0.1', 50052)); f = s.makefile('rwb')
f.write(b'{\"op\":\"get_mode\"}\n'); f.flush(); print(f.readline())
"
```

Or exercise the exact C++ client path the addon uses:

```bash
g++ -std=c++17 -Ifcitx5/src fcitx5/src/pskkjson.cpp fcitx5/src/pskkclient.cpp \
    fcitx5/tools/json_smoke.cpp -o /tmp/pskk_smoke && /tmp/pskk_smoke
```

## Development workflow

```bash
cargo build --bin pskk-server        # after changing the Rust engine/server
pkill pskk-server || true            # addon auto-starts the new server next time

just fcitx5-build                    # rebuild pskk.so
fcitx5-remote -r                     # restart Fcitx (reloads the addon)
```

Server discovery (same as the IBus client): if nothing listens on
`PSKK_JSON_ADDR`, the addon forks `pskk-server` searching, in order:
`$PSKK_SERVER`, `$PSKK_INSTALL_ROOT/bin/pskk-server`,
`/opt/pskk/bin/pskk-server`, `./target/{release,debug}/pskk-server`,
`/usr/local/bin/pskk-server`.

### Compile-checking the addon without installing Fcitx

The engine targets the fcitx5 API as of **5.1.x** (`InputMethodEngineV2`, the
`fcitx::Action` menu model). With the fcitx5 development packages installed
the normal cmake build covers this; without them, the sources can still be
syntax-checked against fetched headers (see the commit message/history of
`fcitx5/src` for the exact `-I` recipe used during development).

## Behavioral parity with the IBus client

The addon deliberately mirrors `ibus-engine-pskk.py` 1:1:

| IBus client                                     | Fcitx addon                                                       |
|-------------------------------------------------|-------------------------------------------------------------------|
| `IBus.Engine` process                           | `fcitx::InputMethodEngineV2` addon loaded in the fcitx5 daemon     |
| `do_process_key_event` + X keyvals/modifiers    | `keyEvent()` + `KeyEvent::rawKey()` (post-layout, pre-normalize)   |
| Super-key manual tracking & passthrough         | same, via `Super_L`/`Super_R` key names                            |
| `commit_text` / `update_preedit_text` (attrs)   | `InputContext::commitString` + `Text` with Underline/HighLight     |
| `IBus.LookupTable` page 9                       | `CommonCandidateList` page size 9                                 |
| IBus property menu `InputMode` (`あ`/`A`)       | `SimpleAction` menu + `subModeLabelImpl` (`あ`/`A`)               |
| `Settings` / `Dictionary Editor` / `IME Tester` property items | menu actions that exec the same apps                              |
| `do_focus_out` → server `FocusOut`              | `deactivate()` → server `focus_out`                               |
| `do_reset` → server `Reset`                     | `reset()` → server `reset`                                        |
| Henkan-load retry (10 s in own process)         | bounded 1 s retry + key suppression (in-process daemon safety)     |
| server auto-start / reconnect                   | identical logic in C++ (`pskkclient.cpp`)                          |

### Known differences and limitations

- **In-process execution**: IBus runs the Python client as its own process, so
  its blocking calls only stall the engine. The fcitx addon runs inside the
  fcitx5 daemon, so RPCs are kept synchronous-but-bounded (short socket
  timeouts, bounded load retry) and server warm-up happens on a background
  thread. If key latency ever becomes an issue, move the RPCs to a worker
  thread and marshal results onto the fcitx event loop.
- **Candidate clicks**: not handled, exactly like the IBus client (selection
  is key-driven). Candidate words are `DisplayOnlyCandidateWord`s.
- **Sub-mode indicator**: mirrors the IBus client's `あ`/`A` property symbol.
  The engine reports `subModeIconImpl` (`pskk-hiragana` / `pskk-alphanumeric`
  icons installed into the hicolor theme) and `subModeLabelImpl` (`あ`/`A`);
  fcitx uses these for the tray/status indicator
  (`Instance::inputMethodIcon/Label`). The engine fires a `StatusArea` UI
  update on every mode change so the indicator repaints immediately.
- **Client preedit**: when the application supports it (`CapabilityFlag::
  Preedit`, e.g. GTK/Qt frontends), preedit is rendered *inline* by the app
  with underline/highlight, exactly like other fcitx5 IMEs (fcitx5-skk/mozc).
  Only clients without inline-preedit support fall back to the floating panel
  window. Caveat: not every toolkit renders the full fcitx5 preedit styling;
  GTK3 underlines but may ignore some advanced format flags.
- **Shared engine state**: `pskk-server` keeps one global engine instance for
  all clients (same design as IBus). Typing in two windows concurrently shares
  preedit state; per-window state would require keying by input context.
- Both frontends can share one running server, but a desktop session must use
  one input framework at a time.

## Troubleshooting

- **PSKK missing in fcitx5-configtool** → check the addon registered:
  `ls /usr/share/fcitx5/addon/pskk.conf /usr/share/fcitx5/inputmethod/pskk.conf`
  and that `fcitx5 -rd` logs no load error for `pskk`.
- **No Japanese output / keys pass through** → is the mode `A`? Toggle with
  `Ctrl+J`/`Ctrl+\` or via the menu. Check `pskk-server` state with the JSON
  probe above (`get_mode` should read `{"mode":1}` in Hiragana).
- **Server does not start** → run `$PSKK_SERVER` (or
  `/opt/pskk/bin/pskk-server`) manually and watch its log at
  `~/.config/pskk/pskk.log`; also see the path list above. `PSKK_JSON_ADDR`
  lets you point the addon at a dev server on another port.
- **Full diagnostics**: `fcitx5-diagnose | grep -A5 -i pskk`.
