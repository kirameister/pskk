import { useState } from "react";
import { predict } from "../api";
import type {
  EnvironmentInfo,
  ModelInfo,
  PredictionResult,
  ResultViewMode,
} from "../types";
import ModelPicker from "./ModelPicker";
import NBestPanel from "./NBestPanel";

interface TestViewProps {
  env: EnvironmentInfo | null;
  models: ModelInfo[];
  modelPath: string;
  onModelPathChange: (path: string) => void;
  onReloadModels: () => void;
}

const EXAMPLES = ["きょうはてんきがよい", "わたしはがくせいです", "とうきょうにいきたい"];

/**
 * Test tab: predict bunsetsu splits for arbitrary input.
 * テストタブ: 任意の入力に対して文節分割を予測。
 */
export default function TestView({
  env,
  models,
  modelPath,
  onModelPathChange,
  onReloadModels,
}: TestViewProps) {
  const [inputText, setInputText] = useState("きょうはてんきがよい");
  const [nBest, setNBest] = useState(5);
  const [debug, setDebug] = useState(false);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<PredictionResult | null>(null);
  const [activeIndex, setActiveIndex] = useState(0);
  const [mode, setMode] = useState<ResultViewMode>("canvas");

  const runPrediction = async () => {
    setRunning(true);
    setError(null);
    try {
      const prediction = await predict({
        inputText,
        modelPath: modelPath || null,
        nBest,
        debug,
      });
      setResult(prediction);
      setActiveIndex(0);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setResult(null);
    } finally {
      setRunning(false);
    }
  };

  const noModel = models.length === 0 && !modelPath;

  return (
    <div className="view">
      <section className="panel">
        <header className="panel-head">
          <h2>Test bunsetsu-split prediction</h2>
        </header>

        <div className="test-input-row">
          <input
            className="input input-lg"
            type="text"
            value={inputText}
            placeholder="e.g., きょうはてんきがよい"
            onChange={(event) => setInputText(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !running && inputText.trim()) runPrediction();
            }}
          />
          <button
            type="button"
            className="btn btn-primary btn-lg"
            disabled={running || !inputText.trim()}
            onClick={runPrediction}
          >
            {running ? "Predicting…" : "Predict"}
          </button>
        </div>

        <div className="hint-row">
          <span className="muted">Examples:</span>
          {EXAMPLES.map((example) => (
            <button
              key={example}
              type="button"
              className="chip"
              onClick={() => setInputText(example)}
            >
              {example}
            </button>
          ))}
        </div>

        <div className="control-grid">
          <ModelPicker
            models={models}
            value={modelPath}
            onChange={onModelPathChange}
            onReload={onReloadModels}
            defaultModelPath={env?.defaultModelPath ?? "—"}
            disabled={running}
          />

          <div className="field">
            <label className="field-label" htmlFor="nbest">
              N-best results
            </label>
            <div className="field-row">
              <input
                id="nbest"
                className="input"
                type="number"
                min={1}
                max={10}
                value={nBest}
                disabled={running}
                onChange={(event) =>
                  setNBest(Math.max(1, Math.min(10, Number(event.target.value) || 1)))
                }
              />
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={debug}
                  disabled={running}
                  onChange={(event) => setDebug(event.target.checked)}
                />
                Debug detail (all M×M transitions + feature weights)
              </label>
            </div>
            <p className="field-hint">
              Hiragana input only — the model predicts boundaries over yomi.
            </p>
          </div>
        </div>

        {noModel && (
          <p className="alert alert-warn">
            No trained model was found. Train one in the <strong>Train</strong> tab, then hit
            Rescan.
          </p>
        )}
        {error && <p className="alert alert-error">{error}</p>}
      </section>

      {result && (
        <>
          <div className="summary-row">
            <div className="summary-card">
              <span className="summary-label">Model</span>
              <span className="summary-value mono">{result.modelPath || "—"}</span>
            </div>
            <div className="summary-card">
              <span className="summary-label">Tokens</span>
              <span className="summary-value">{result.tokens.length}</span>
            </div>
            <div className="summary-card">
              <span className="summary-label">Labels in model</span>
              <span className="summary-value">{result.modelLabels.length}</span>
            </div>
            <div className="summary-card">
              <span className="summary-label">Candidates</span>
              <span className="summary-value">{result.candidates.length}</span>
            </div>
            <div className="summary-card">
              <span className="summary-label">Transitions shown</span>
              <span className="summary-value">{result.transitions.length}</span>
            </div>
          </div>

          <NBestPanel
            result={result}
            mode={mode}
            onModeChange={setMode}
            activeIndex={activeIndex}
            onActiveIndexChange={setActiveIndex}
          />
        </>
      )}
    </div>
  );
}
