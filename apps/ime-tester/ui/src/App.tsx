import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

interface PreeditSegment {
  text: string;
  is_selected: boolean;
}

interface Candidate {
  surface: string;
  reading: string;
  count: number;
  passthrough?: boolean;
  bunsetsu_mode?: boolean;
}

interface EngineOutput {
  commit_string: string | null;
  preedit_segments: PreeditSegment[];
  preedit_cursor_pos: number;
  candidates: Candidate[];
  candidate_cursor_pos: number;
  show_candidates: boolean;
  consumed: boolean;
  current_mode: number; // 0 = ALPHANUMERIC, 1 = HIRAGANA
  marker_state: number; // 0 = IDLE, 1 = MARKER_HELD, 2 = FIRST_PRESSED, 3 = FIRST_RELEASED, 4 = KANCHOKU_SECOND_PRESSED
}

type InputMode = "A" | "あ";

function App() {
  const [mode, setMode] = useState<InputMode>("A");
  const [committedText, setCommittedText] = useState("");
  const [preeditSegments, setPreeditSegments] = useState<PreeditSegment[]>([]);
  const [candidates, setCandidates] = useState<Candidate[]>([]);
  const [candidateCursor, setCandidateCursor] = useState(0);
  const [logs, setLogs] = useState<string[]>([]);
  const [connected, setConnected] = useState(false);
  const [connecting, setConnecting] = useState(false);
  const [logOrderNewestFirst, setLogOrderNewestFirst] = useState(true);
  const [markerState, setMarkerState] = useState<string>("Idle");
  const [selectedLogIndices, setSelectedLogIndices] = useState<Set<number>>(new Set());
  const [isDragging, setIsDragging] = useState(false);
  const [dragStartIndex, setDragStartIndex] = useState<number | null>(null);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number } | null>(null);

  const addLog = useCallback((message: string) => {
    const timestamp = new Date().toLocaleTimeString();
    setLogs((prev) => [...prev, `[${timestamp}] ${message}`]);
  }, []);

  const handleLogMouseDown = useCallback((index: number, event: React.MouseEvent) => {
    if (event.button === 0) { // Only left-click triggers selection
      setIsDragging(true);
      setDragStartIndex(index);
      setSelectedLogIndices(new Set([index]));
    }
  }, []);

  const handleLogMouseEnter = useCallback((index: number) => {
    if (isDragging && dragStartIndex !== null) {
      const start = Math.min(dragStartIndex, index);
      const end = Math.max(dragStartIndex, index);
      const newSet = new Set<number>();
      for (let i = start; i <= end; i++) {
        newSet.add(i);
      }
      setSelectedLogIndices(newSet);
    }
  }, [isDragging, dragStartIndex]);

  const handleLogMouseUp = useCallback(() => {
    setIsDragging(false);
    setDragStartIndex(null);
  }, []);

  const handleLogRightClick = useCallback((event: React.MouseEvent) => {
    event.preventDefault();
    setContextMenu({ x: event.clientX, y: event.clientY });
  }, []);

  const copySelectedLogs = useCallback(() => {
    const selectedLogs = (logOrderNewestFirst ? [...logs].reverse() : logs)
      .filter((_, index) => selectedLogIndices.has(index));
    const textToCopy = selectedLogs.join('\n');
    navigator.clipboard.writeText(textToCopy);
    setContextMenu(null);
  }, [logs, selectedLogIndices, logOrderNewestFirst]);

  const clearLogSelection = useCallback(() => {
    setSelectedLogIndices(new Set());
  }, []);
  
  const handleEngineOutput = useCallback((output: EngineOutput, addLog: (msg: string) => void) => {
    addLog(`Processing output: commit_string=${output.commit_string ? 'yes' : 'no'}, preedit_segments=${output.preedit_segments.length}`);

    if (output.commit_string) {
      setCommittedText((prev) => prev + output.commit_string);
      addLog(`Committed: "${output.commit_string}"`);
    }

    addLog(`Setting preedit segments: ${output.preedit_segments.length} segments`);
    setPreeditSegments(output.preedit_segments);
    setCandidates(output.candidates);
    setCandidateCursor(output.candidate_cursor_pos);

    // Sync mode from engine output (0 = ALPHANUMERIC, 1 = HIRAGANA)
    const newMode: InputMode = output.current_mode === 0 ? "A" : "あ";
    setMode(newMode);

    // Sync marker state from engine output (0 = IDLE, 1 = MARKER_HELD, 2 = FIRST_PRESSED, 3 = FIRST_RELEASED, 4 = KANCHOKU_SECOND_PRESSED)
    const markerStateMap: Record<number, string> = {
      0: "Idle",
      1: "MarkerHeld",
      2: "FirstPressed",
      3: "FirstReleased",
      4: "KanchokuSecondPressed",
    };
    setMarkerState(markerStateMap[output.marker_state] ?? "Idle");

    if (output.preedit_segments.length > 0) {
      const preeditText = output.preedit_segments.map(s => s.text).join('');
      addLog(`Preedit: "${preeditText}"`);
    }

    if (output.show_candidates && output.candidates.length > 0) {
      addLog(`Candidates: ${output.candidates.length} options`);
    }
  }, []);

  const handleModeChange = useCallback(async (newMode: InputMode) => {
    addLog(`Changing mode to: ${newMode}`);
    try {
      // Convert to protobuf enum: 0 = Alphanumeric, 1 = Hiragana
      const modeValue = newMode === "A" ? 0 : 1;
      const output = await invoke<EngineOutput>("set_mode", { mode: modeValue });
      addLog(`Mode changed successfully to: ${newMode}`);
      setMode(newMode);
      handleEngineOutput(output, addLog);
    } catch (error) {
      addLog(`Failed to set mode: ${error}`);
      console.error("Failed to set mode:", error);
    }
  }, [handleEngineOutput, addLog]);

  // Normalize key name to platform-independent format (same logic as settings app)
  const normalizeKeyName = useCallback((e: KeyboardEvent): string => {
    const modifiers: string[] = [];
    if (e.ctrlKey) modifiers.push("Control");
    if (e.altKey) modifiers.push("Alt");
    if (e.shiftKey) modifiers.push("Shift");
    if (e.metaKey) modifiers.push("Meta");

    let key = e.key;

    // Normalize special keys to readable names
    const keyNormalizations: { [key: string]: string } = {
      " ": "Space",
      "Enter": "Enter",
      "Tab": "Tab",
      "Escape": "Escape",
      "Backspace": "Backspace",
      "Delete": "Delete",
      "ArrowUp": "ArrowUp",
      "ArrowDown": "ArrowDown",
      "ArrowLeft": "ArrowLeft",
      "ArrowRight": "ArrowRight",
    };

    if (keyNormalizations[key]) {
      key = keyNormalizations[key];
    }

    // Build the full key string (e.g., "Control+Shift+K")
    const keyParts = [...modifiers];
    if (key && key !== "Control" && key !== "Alt" && key !== "Shift" && key !== "Meta") {
      keyParts.push(key);
    }

    return keyParts.join("+");
  }, []);

  const handleKeyEvent = useCallback(async (e: KeyboardEvent, isPressed: boolean) => {
    addLog(`Key event: key="${e.key}" code="${e.code}" isPressed=${isPressed}`);

    // Handle ESC key to exit in direct-input mode (A button)
    if (e.key === "Escape" && isPressed && mode === "A") {
      addLog("ESC pressed in direct-input mode, exiting...");
      if (typeof invoke === 'function') {
        await invoke("close_window");
      }
      return;
    }

    // Handle Ctrl+J for mode toggle
    if (e.ctrlKey && e.key === "j") {
      e.preventDefault();
      const newMode = mode === "A" ? "あ" : "A";
      handleModeChange(newMode);
      return;
    }

    // Process both keydown and keyup events for marker key state machine
    // The space key release is needed to trigger bunsetsu mode decision

    const keyChar = e.key.length === 1 ? e.key : null;
    const keyName = normalizeKeyName(e);
    const modifiers = {
      shift: e.shiftKey,
      ctrl: e.ctrlKey,
      alt: e.altKey,
    };

    addLog(`Invoking backend: keyName="${keyName}" keyChar=${keyChar}`);

    try {
      if (typeof invoke !== 'function') {
        console.error('Tauri invoke not available');
        return;
      }
      
      const output = await invoke<EngineOutput>("process_key", {
        keyChar,
        keyName,
        isPressed,
        modifiers,
      });

      addLog(`Backend returned: consumed=${output.consumed}, preedit_segments=${output.preedit_segments.length}`);

      if (keyChar) {
        addLog(`Key pressed: "${keyChar}" (consumed: ${output.consumed})`);
      }

      if (output.consumed) {
        e.preventDefault();
      }

      handleEngineOutput(output, addLog);
    } catch (error) {
      addLog(`Key processing error: ${error}`);
      console.error("Failed to process key:", error);
    }
  }, [handleEngineOutput, mode, handleModeChange, addLog]);
  
  const connectToServer = useCallback(async () => {
    if (connecting || connected) return;

    setConnecting(true);
    addLog("Connecting to PSKK server at 127.0.0.1:50051...");

    try {
      const result = await invoke<string>("connect_to_server");
      addLog(result);
      setConnected(true);
      setConnecting(false);

      // Load mode after connection
      await loadMode();

      // Get and log dictionary size
      try {
        const dictSize = await invoke<number>("get_dictionary_size");
        addLog(`Dictionary loaded with ${dictSize} yomi-to-kanji entries`);
      } catch (error) {
        addLog(`Failed to get dictionary size: ${error}`);
      }
    } catch (error) {
      addLog(`Connection failed: ${error}`);
      addLog("Please start the server with: just server-dev");
      setConnected(false);
      setConnecting(false);
    }
  }, [connecting, connected, addLog]);

  useEffect(() => {
    addLog("IME Tester initialized");
    connectToServer();
  }, [addLog, connectToServer]);

  useEffect(() => {
    if (connected) {
      loadConfig();
    }
  }, [connected]);

  const loadMode = async () => {
    try {
      const currentMode = await invoke<number>("get_mode");
      // Convert from protobuf enum: 0 = Alphanumeric, 1 = Hiragana
      setMode(currentMode === 0 ? "A" : "あ");
    } catch (error) {
      addLog(`Failed to get mode: ${error}`);
      console.error("Failed to get mode:", error);
    }
  };

  const loadConfig = async () => {
    try {
      const configJson = await invoke<string>("get_loaded_config");
      addLog("Loaded Config:");
      
      // Split config into chunks for readable log display
      const chunkSize = 500;
      for (let i = 0; i < configJson.length; i += chunkSize) {
        const chunk = configJson.slice(i, i + chunkSize);
        addLog(chunk);
      }
    } catch (error) {
      addLog(`Failed to load config: ${error}`);
      console.error("Failed to load config:", error);
    }
  };


  return (
    <div className="app" onClick={() => {
      // Refocus the invisible input when clicking anywhere in the app
      const input = document.querySelector('input[type="text"]') as HTMLInputElement;
      if (input) input.focus();
    }}>
      {/* Invisible input to capture keyboard events */}
      <input
        type="text"
        autoFocus
        style={{
          position: 'absolute',
          opacity: 0,
          width: 0,
          height: 0,
          pointerEvents: 'none',
        }}
        onKeyDown={(e) => handleKeyEvent(e.nativeEvent, true)}
        onKeyUp={(e) => handleKeyEvent(e.nativeEvent, false)}
      />

      <div className="content-wrapper">
        <aside className="log-pane">
          <div className="log-header">
            <div className="log-header-left">
              <span>Event Log</span>
              <span className={`connection-status ${connected ? "connected" : "disconnected"}`}>
                {connecting ? "Connecting..." : connected ? "● Connected" : "● Disconnected"}
              </span>
            </div>
            <div className="log-header-right">
              <button
                className={`log-order-button ${logOrderNewestFirst ? "selected" : ""}`}
                onClick={() => setLogOrderNewestFirst(true)}
                title="Newest log at top"
              >
                △
              </button>
              <button
                className={`log-order-button ${!logOrderNewestFirst ? "selected" : ""}`}
                onClick={() => setLogOrderNewestFirst(false)}
                title="Oldest log at top"
              >
                ▽
              </button>
              <button className="clear-log-button" onClick={() => setLogs([])}>Clear</button>
            </div>
          </div>
          <div className="log-content">
            {logs.length === 0 ? (
              <div className="log-empty">No events logged yet</div>
            ) : (
              (logOrderNewestFirst ? [...logs].reverse() : logs).map((log, i) => (
                <div
                  key={i}
                  className={`log-entry ${selectedLogIndices.has(i) ? 'selected' : ''}`}
                  onMouseDown={(e) => handleLogMouseDown(i, e)}
                  onMouseEnter={() => handleLogMouseEnter(i)}
                  onMouseUp={handleLogMouseUp}
                  onContextMenu={handleLogRightClick}
                >
                  {log}
                </div>
              ))
            )}
          </div>
          {contextMenu && (
            <div
              className="context-menu"
              style={{ left: contextMenu.x, top: contextMenu.y }}
              onClick={() => setContextMenu(null)}
            >
              <div className="context-menu-item" onClick={copySelectedLogs}>
                Copy Selected Logs
              </div>
              <div className="context-menu-item" onClick={clearLogSelection}>
                Clear Selection
              </div>
            </div>
          )}
        </aside>

        <div className="main-content">
        <div className="section">
          <div className="section-title-row">
            <div className="section-title">Preedit Display</div>
            <div className="mode-switcher">
              <button
                className={`mode-button ${mode === "A" ? "active" : ""}`}
                onClick={() => handleModeChange("A")}
                disabled={!connected}
              >
                A
              </button>
              <button
                className={`mode-button ${mode === "あ" ? "active" : ""}`}
                onClick={() => handleModeChange("あ")}
                disabled={!connected}
              >
                あ
              </button>
            </div>
          </div>
          <div className="preedit-display">
            {preeditSegments.length > 0 ? (
              preeditSegments.map((segment, i) => (
                <span
                  key={i}
                  className={`preedit-segment ${
                    segment.is_selected ? "selected" : ""
                  }`}
                >
                  {segment.text}
                </span>
              ))
            ) : (
              <span className="preedit-empty">No preedit</span>
            )}
          </div>
        </div>

        <div className="section">
          <div className="section-title">Committed Output</div>
          <div className="output-display">
            {committedText || <span className="output-empty">No output yet</span>}
          </div>
        </div>

        <div className="section">
          <div className="state-grid">
            <span className="state-label">Marker State:</span>
            <span className="state-value">{markerState}</span>
          </div>
          
          <div className="section-title" style={{ marginTop: '16px' }}>Conversion Candidates</div>
          <div className="candidates-table">
            {(() => {
              // Calculate sliding window: show 5 candidates at a time
              const maxVisible = 5;
              const startIdx = Math.max(0, Math.min(
                candidateCursor - Math.floor(maxVisible / 2),
                candidates.length - maxVisible
              ));
              const endIdx = Math.min(startIdx + maxVisible, candidates.length);
              const visibleCandidates = candidates.slice(startIdx, endIdx);
              
              return visibleCandidates.length > 0 ? (
                visibleCandidates.map((candidate, i) => {
                  const actualIdx = startIdx + i;
                  return (
                    <div
                      key={actualIdx}
                      className={`candidate-row ${
                        actualIdx === candidateCursor ? "selected" : ""
                      }`}
                    >
                      {candidate.surface}
                    </div>
                  );
                })
              ) : (
                // Show 5 empty placeholder rows when no candidates
                Array.from({ length: maxVisible }).map((_, i) => (
                  <div key={i} className="candidate-row empty">
                    &nbsp;
                  </div>
                ))
              );
            })()}
          </div>
        </div>
        </div>
      </div>
    </div>
  );
}

export default App;
