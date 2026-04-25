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
}

type InputMode = "A" | "あ";

function App() {
  const [mode, setMode] = useState<InputMode>("A");
  const [committedText, setCommittedText] = useState("");
  const [preeditSegments, setPreeditSegments] = useState<PreeditSegment[]>([]);
  const [candidates, setCandidates] = useState<Candidate[]>([]);
  const [showCandidates, setShowCandidates] = useState(false);
  const [candidateCursor, setCandidateCursor] = useState(0);
  const [dictionaryLoaded, setDictionaryLoaded] = useState(false);
  const [logs, setLogs] = useState<string[]>([]);
  const [connected, setConnected] = useState(false);
  const [connecting, setConnecting] = useState(false);
  
  const addLog = useCallback((message: string) => {
    const timestamp = new Date().toLocaleTimeString();
    setLogs((prev) => [...prev, `[${timestamp}] ${message}`]);
  }, []);
  
  const handleEngineOutput = useCallback((output: EngineOutput, addLog: (msg: string) => void) => {
    if (output.commit_string) {
      setCommittedText((prev) => prev + output.commit_string);
      addLog(`Committed: "${output.commit_string}"`);
    }

    setPreeditSegments(output.preedit_segments);
    setCandidates(output.candidates);
    setShowCandidates(output.show_candidates);
    setCandidateCursor(output.candidate_cursor_pos);
    
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

  const handleKeyEvent = useCallback(async (e: KeyboardEvent, isPressed: boolean) => {
    // Handle mode toggle shortcut (Ctrl+Space)
    if (e.ctrlKey && e.key === ' ' && isPressed) {
      e.preventDefault();
      const newMode = mode === "A" ? "あ" : "A";
      handleModeChange(newMode);
      return;
    }

    // Only process keydown events, skip keyup to prevent preedit clearing
    if (!isPressed) {
      return;
    }

    const keyChar = e.key.length === 1 ? e.key : null;
    const keyName = e.key;
    const modifiers = {
      shift: e.shiftKey,
      ctrl: e.ctrlKey,
      alt: e.altKey,
    };

    // Debug: Log all key presses
    console.log(`Key event: key="${e.key}" code="${e.code}" isPressed=${isPressed}`);

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
    
    // Add window-level keyboard event listeners
    const handleWindowKeyDown = async (e: KeyboardEvent) => {
      await handleKeyEvent(e, true);
    };
    
    const handleWindowKeyUp = async (e: KeyboardEvent) => {
      await handleKeyEvent(e, false);
    };
    
    window.addEventListener('keydown', handleWindowKeyDown);
    window.addEventListener('keyup', handleWindowKeyUp);
    
    return () => {
      window.removeEventListener('keydown', handleWindowKeyDown);
      window.removeEventListener('keyup', handleWindowKeyUp);
    };
  }, [handleKeyEvent, addLog, connectToServer]);

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

  const handleClear = () => {
    setCommittedText("");
    setPreeditSegments([]);
    setCandidates([]);
    setShowCandidates(false);
  };

  const handleLoadDictionary = async () => {
    try {
      const result = await invoke<string>("load_sample_dictionary");
      setDictionaryLoaded(true);
      alert(result);
    } catch (error) {
      console.error("Failed to load dictionary:", error);
      alert("Failed to load dictionary: " + error);
    }
  };

  const preeditText = preeditSegments.map((seg) => seg.text).join("");

  return (
    <div className="app">
      <div className="header">
        <div className="header-left">
          <h1>PSKK IME Tester</h1>
          <span className={`connection-status ${connected ? "connected" : "disconnected"}`}>
            {connecting ? "Connecting..." : connected ? "● Connected" : "● Disconnected"}
          </span>
        </div>
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

      <div className="content-wrapper">
        <aside className="log-pane">
          <div className="log-header">
            <span>Event Log</span>
            <button className="clear-log-button" onClick={() => setLogs([])}>Clear</button>
          </div>
          <div className="log-content">
            {logs.length === 0 ? (
              <div className="log-empty">No events logged yet</div>
            ) : (
              logs.map((log, i) => (
                <div key={i} className="log-entry">{log}</div>
              ))
            )}
          </div>
        </aside>

        <div className="main-content">
        <div className="section">
          <div className="section-title">Preedit Display</div>
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

        {showCandidates && (
          <div className="section">
            <div className="section-title">Candidates</div>
            <div className="candidates-list">
              {candidates.length > 0 ? (
                candidates.map((candidate, i) => (
                  <div
                    key={i}
                    className={`candidate-item ${
                      i === candidateCursor ? "selected" : ""
                    }`}
                  >
                    <div>
                      <span className="candidate-surface">{candidate.surface}</span>
                      <span className="candidate-reading">
                        ({candidate.reading})
                      </span>
                    </div>
                    <span className="candidate-count">[{candidate.count}]</span>
                  </div>
                ))
              ) : (
                <div className="candidates-empty">No candidates</div>
              )}
            </div>
          </div>
        )}

        <div className="section">
          <div className="section-title">Engine State</div>
          <div className="state-grid">
            <span className="state-label">Mode:</span>
            <span className="state-value">{mode}</span>

            <span className="state-label">Preedit:</span>
            <span className="state-value">{preeditText || "(empty)"}</span>

            <span className="state-label">Show Candidates:</span>
            <span className={`state-value ${showCandidates}`}>
              {showCandidates.toString()}
            </span>

            <span className="state-label">Candidate Count:</span>
            <span className="state-value">{candidates.length}</span>

            <span className="state-label">Dictionary Loaded:</span>
            <span className={`state-value ${dictionaryLoaded}`}>
              {dictionaryLoaded.toString()}
            </span>
          </div>
        </div>

        <div className="section">
          <div className="section-title">Actions</div>
          <div className="actions">
            <button className="action-button" onClick={handleClear}>
              Clear
            </button>
            <button
              className="action-button secondary"
              onClick={handleLoadDictionary}
              disabled={dictionaryLoaded}
            >
              {dictionaryLoaded ? "Dictionary Loaded" : "Load Sample Dictionary"}
            </button>
          </div>
        </div>
        </div>
      </div>
    </div>
  );
}

export default App;
