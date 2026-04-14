import { useState, useRef, useEffect } from "react";
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

type InputMode = "Alphanumeric" | "Hiragana";

function App() {
  const [mode, setMode] = useState<InputMode>("Alphanumeric");
  const [committedText, setCommittedText] = useState("");
  const [preeditSegments, setPreeditSegments] = useState<PreeditSegment[]>([]);
  const [candidates, setCandidates] = useState<Candidate[]>([]);
  const [showCandidates, setShowCandidates] = useState(false);
  const [candidateCursor, setCandidateCursor] = useState(0);
  const [dictionaryLoaded, setDictionaryLoaded] = useState(false);
  
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    loadMode();
  }, []);

  const loadMode = async () => {
    try {
      const currentMode = await invoke<InputMode>("get_mode");
      setMode(currentMode);
    } catch (error) {
      console.error("Failed to get mode:", error);
    }
  };

  const handleModeChange = async (newMode: InputMode) => {
    try {
      const output = await invoke<EngineOutput>("set_mode", { mode: newMode });
      setMode(newMode);
      handleEngineOutput(output);
    } catch (error) {
      console.error("Failed to set mode:", error);
    }
  };

  const handleEngineOutput = (output: EngineOutput) => {
    if (output.commit_string) {
      setCommittedText((prev) => prev + output.commit_string);
    }

    setPreeditSegments(output.preedit_segments);
    setCandidates(output.candidates);
    setShowCandidates(output.show_candidates);
    setCandidateCursor(output.candidate_cursor_pos);
  };

  const handleKeyDown = async (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    const keyChar = e.key.length === 1 ? e.key : null;
    const keyName = e.key;
    const modifiers = {
      shift: e.shiftKey,
      ctrl: e.ctrlKey,
      alt: e.altKey,
    };

    try {
      const output = await invoke<EngineOutput>("process_key", {
        keyChar,
        keyName,
        isPressed: true,
        modifiers,
      });

      if (output.consumed) {
        e.preventDefault();
      }

      handleEngineOutput(output);
    } catch (error) {
      console.error("Failed to process key:", error);
    }
  };

  const handleKeyUp = async (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    const keyChar = e.key.length === 1 ? e.key : null;
    const keyName = e.key;
    const modifiers = {
      shift: e.shiftKey,
      ctrl: e.ctrlKey,
      alt: e.altKey,
    };

    try {
      const output = await invoke<EngineOutput>("process_key", {
        keyChar,
        keyName,
        isPressed: false,
        modifiers,
      });

      if (output.consumed) {
        e.preventDefault();
      }

      handleEngineOutput(output);
    } catch (error) {
      console.error("Failed to process key:", error);
    }
  };

  const handleClear = () => {
    setCommittedText("");
    setPreeditSegments([]);
    setCandidates([]);
    setShowCandidates(false);
    if (textareaRef.current) {
      textareaRef.current.focus();
    }
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
        <h1>PSKK IME Tester</h1>
        <div className="mode-switcher">
          <button
            className={`mode-button ${mode === "Alphanumeric" ? "active" : ""}`}
            onClick={() => handleModeChange("Alphanumeric")}
          >
            A
          </button>
          <button
            className={`mode-button ${mode === "Hiragana" ? "active" : ""}`}
            onClick={() => handleModeChange("Hiragana")}
          >
            あ
          </button>
        </div>
      </div>

      <div className="main-content">
        <div className="section">
          <div className="section-title">Input Area</div>
          <textarea
            ref={textareaRef}
            className="input-area"
            value={committedText}
            onChange={(e) => setCommittedText(e.target.value)}
            onKeyDown={handleKeyDown}
            onKeyUp={handleKeyUp}
            placeholder="Type here... (Switch to あ mode to test IME)"
            autoFocus
          />
          <div className="help-text">
            <strong>Quick Guide:</strong>
            Switch to あ mode, type romaji (e.g., "ai"), press Space to convert, use
            arrows to navigate candidates, Enter to confirm, Escape to cancel.
          </div>
        </div>

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
  );
}

export default App;
