# PSKK IME Tester

A platform-independent testing interface for the PSKK Japanese IME engine built with Tauri.

## Purpose

This app allows you to test and debug the IME engine behavior without needing to:
- Restart IBus or platform IME services
- Deal with platform-specific integration issues
- Lose debug output in system logs

## Features

- ✅ **Real keyboard event capture** - Test actual typing behavior
- ✅ **Visual preedit display** - See bunsetsu segmentation and selection
- ✅ **Candidate window** - View and navigate conversion candidates
- ✅ **Engine state inspection** - Monitor internal state in real-time
- ✅ **Cross-platform** - Works on Linux, Windows, macOS
- ✅ **Hot reload** - Instant UI updates during development

## Running the App

### Development Mode

```bash
# From the ime-tester directory
cd apps/ime-tester

# Install frontend dependencies (first time only)
npm --prefix ui install

# Run in development mode
cargo tauri dev
```

### Building

```bash
cargo tauri build
```

## Usage

1. **Switch to Hiragana mode** - Click the "あ" button
2. **Type romaji** - e.g., "ai" → "あい"
3. **Press Space** - Trigger conversion
4. **Navigate candidates**:
   - ↓/↑ - Next/previous candidate
   - ←/→ - Previous/next bunsetsu (bunsetsu mode only)
5. **Confirm** - Press Enter
6. **Cancel** - Press Escape

## Testing Scenarios

### Basic Conversion
```
Type: "ai"
Press: Space
Result: Shows candidates ["愛", "相", "藍"]
Press: Enter
Result: Commits "愛"
```

### Bunsetsu Mode
```
Type: "kyouhatenkigayoi"
Press: Space
Result: If no whole-word match, segments into bunsetsu
Navigate: Use arrows to select bunsetsu and cycle candidates
```

### Mode Switching
```
Click: "A" button
Type: "hello"
Result: Passes through as-is (no conversion)
```

## Dictionary

The app includes a sample dictionary with common words:
- へんかん → 変換, 返還, 編纂
- きょう → 今日, 京, 教
- てんき → 天気, 転機
- あい → 愛, 相, 藍
- etc.

Click "Load Sample Dictionary" to activate it.

## Architecture

```
┌─────────────────────────────────────┐
│         React UI (TypeScript)       │
│  - Keyboard event capture           │
│  - Visual rendering                 │
│  - State display                    │
└──────────────┬──────────────────────┘
               │ Tauri IPC
               │
┌──────────────▼──────────────────────┐
│      Rust Backend (Tauri)           │
│  - PSKKEngine wrapper               │
│  - Tauri commands                   │
│  - State management                 │
└──────────────┬──────────────────────┘
               │
┌──────────────▼──────────────────────┐
│        PSKKEngine (pskk crate)      │
│  - Core IME logic                   │
│  - Platform-independent             │
└─────────────────────────────────────┘
```

## Development Tips

- **Console logs** - Check browser DevTools for frontend logs
- **Rust logs** - Check terminal for backend logs
- **Hot reload** - UI changes reload automatically
- **Breakpoints** - Use browser DevTools or Rust debugger
- **State inspection** - Watch the "Engine State" panel in real-time

## Comparison with Platform IME

| Feature | Platform IME | IME Tester |
|---------|-------------|------------|
| Restart needed | Yes | No |
| Debug logs | System logs | Browser console |
| State visibility | Hidden | Full visibility |
| Iteration speed | Slow | Fast |
| Cross-platform | Platform-specific | Works everywhere |

## Next Steps

Once the engine behavior is validated here, you can:
1. Implement the IBus adapter with confidence
2. Port to other platforms (Windows TSF, macOS IMK)
3. Use this as a regression testing tool
