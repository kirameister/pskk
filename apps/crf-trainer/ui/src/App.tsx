import { useCallback, useEffect, useState } from "react";
import { getEnvironment, listModels } from "./api";
import StatsView from "./components/StatsView";
import TestView from "./components/TestView";
import TrainView from "./components/TrainView";
import { useEscapeToClose } from "./hooks/useEscapeToClose";
import type { EnvironmentInfo, ModelInfo, ViewId } from "./types";

const NAV: { id: ViewId; label: string; hint: string }[] = [
  { id: "test", label: "Test", hint: "Predict bunsetsu splits" },
  { id: "train", label: "Train", hint: "Browse → extract → train" },
  { id: "stats", label: "Corpus Stats", hint: "Corpus & feature TSV inspection" },
];

/**
 * PSKK CRF Trainer — Tauri shell.
 *
 * PSKK CRFトレーナー — Tauriシェル。
 * Reimagines the GTK `ConversionModelPanel` as a multi-view desktop app and
 * targets the eventual canvas-based visualisation of CRF predictions.
 */
export default function App() {
  const [view, setView] = useState<ViewId>("test");
  const [env, setEnv] = useState<EnvironmentInfo | null>(null);
  const [envError, setEnvError] = useState<string | null>(null);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [modelPath, setModelPath] = useState("");

  // Escape closes the window, matching the GTK panel.
  useEscapeToClose();

  const reloadModels = useCallback(async () => {
    try {
      const found = await listModels();
      setModels(found);
      // Pre-select the default model the first time models appear.
      setModelPath((current) => current || found.find((m) => m.isDefault)?.path || found[0]?.path || "");
    } catch (err) {
      setEnvError(err instanceof Error ? err.message : String(err));
    }
  }, []);

  useEffect(() => {
    getEnvironment()
      .then(setEnv)
      .catch((err) => setEnvError(err instanceof Error ? err.message : String(err)));
    void reloadModels();
  }, [reloadModels]);

  const handleModelsChanged = useCallback((path: string) => {
    setModelPath(path);
  }, []);

  return (
    <div className="app-shell">
      <header className="app-header">
        <div className="brand">
          <span className="brand-mark">CRF</span>
          <div>
            <h1 className="brand-title">PSKK CRF Trainer</h1>
            <p className="brand-sub">Bunsetsu segmentation — train &amp; test</p>
          </div>
        </div>

        <div className="env-badges">
          <span className={`env-badge ${env?.pythonAvailable ? "ok" : "bad"}`}>
            {env?.pythonAvailable ? env.pythonVersion ?? "python ✓" : "python ✗"}
          </span>
          <span
            className={`env-badge ${env?.pycrfsuiteAvailable ? "ok" : "bad"}`}
            title={env?.crfsuiteVersion ? `crfsuite ${env.crfsuiteVersion}` : undefined}
          >
            {env?.pycrfsuiteAvailable ? "pycrfsuite ✓" : "pycrfsuite ✗"}
          </span>
          <span className="env-badge neutral">{models.length} model(s)</span>
        </div>
      </header>

      <div className="app-body">
        <nav className="sidebar">
          {NAV.map((item) => (
            <button
              key={item.id}
              type="button"
              className={`nav-item${view === item.id ? " active" : ""}`}
              onClick={() => setView(item.id)}
            >
              <span className="nav-label">{item.label}</span>
              <span className="nav-hint">{item.hint}</span>
            </button>
          ))}

          <div className="sidebar-foot">
            <span className="muted small">Config dir</span>
            <span className="mono small break-all">{env?.configDir ?? "—"}</span>
          </div>
        </nav>

        <main className="main">
          {envError && <p className="alert alert-error">{envError}</p>}

          {view === "test" && (
            <TestView
              env={env}
              models={models}
              modelPath={modelPath}
              onModelPathChange={setModelPath}
              onReloadModels={reloadModels}
            />
          )}
          {view === "train" && (
            <TrainView
              env={env}
              onReloadModels={reloadModels}
              onModelsChanged={handleModelsChanged}
            />
          )}
          {view === "stats" && <StatsView env={env} />}
        </main>
      </div>

      <footer className="status-bar">
        <span>
          Default model: <span className="mono">{env?.defaultModelPath ?? "—"}</span>
        </span>
        <span>
          Default features TSV: <span className="mono">{env?.defaultFeaturesPath ?? "—"}</span>
        </span>
        <span className="status-hint">
          <kbd>Esc</kbd> close
        </span>
      </footer>
    </div>
  );
}
