# Quick Start Guide

## First Time Setup

```bash
# Navigate to the ime-tester directory
cd apps/ime-tester

# Install frontend dependencies
npm --prefix ui install

# Run the app
cargo tauri dev
```

## What You'll See

1. **A window opens** with the IME Tester interface
2. **Click "Load Sample Dictionary"** to load test data
3. **Click the "あ" button** to switch to Hiragana mode
4. **Type in the input area**: 
   - Type "ai" → you'll see "あい" in the preedit
   - Press Space → conversion candidates appear
   - Press Enter → commits "愛"

## Testing Scenarios

### Basic Conversion
```
1. Switch to あ mode
2. Type: henkan
3. See preedit: へんかん
4. Press: Space
5. See candidates: 変換, 返還, 編纂
6. Press: Enter
7. Result: 変換 committed
```

### Candidate Navigation
```
1. Type: ai
2. Press: Space
3. Press: ↓ (down arrow) to cycle candidates
4. Press: ↑ (up arrow) to go back
5. Press: Enter to confirm
```

### Cancel Conversion
```
1. Type: ai
2. Press: Space
3. Press: Escape
4. Result: Back to preedit "あい"
```

## Troubleshooting

### "Cannot find module" errors
→ Run `npm --prefix ui install`

### Rust compilation errors
→ Make sure you're in the `apps/ime-tester` directory
→ Run `cargo build` from `apps/ime-tester/src-tauri`

### Window doesn't open
→ Check terminal for error messages
→ Ensure port 1421 is not in use

## Next Steps

Once you've validated the engine behavior:
1. Implement the IBus adapter
2. Test on actual IBus
3. Port to other platforms
