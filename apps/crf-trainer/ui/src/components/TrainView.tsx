import { useEffect, useMemo, useRef, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import {
  extractFeatures,
  formatBytes,
  loadTrainingParams,
  onProgress,
  pickCorpusFile,
  pickSaveFile,
  saveTrainingParams,
  trainModel,
} from "../api";
import type {
  EnvironmentInfo,
  FeatureExtractionResult,
  LogLine,
  ProgressEvent,
  TrainingParams,
  TrainingResult,
} from "../types";
import { DEFAULT_TRAINING_PARAMS } from "../types";
import LogConsole from "./LogConsole";
import PipelineStep from "./PipelineStep";

interface TrainViewProps {
  env: EnvironmentInfo | null;
  onReloadModels: () => void;
  onModelsChanged: (path: string) => void;
}

type RunningJob = "extract" | "train" | null;

const numberOr = (raw: string, fallback: number): number => {
  const value = Number(raw);
  return Number.isFinite(value) ? value : fallback;
};

/**
 * Train tab: the three-step browse → extract → train pipeline.
 * トレインタブ: 参照 → 抽出 → 訓練の3ステップパイプライン。
 */
export default function TrainView({ env, onReloadModels, onModelsChanged }: TrainViewProps) {
  const [corpusPaths, setCorpusPaths] = useState<string[]>([]);
  const [featuresPath, setFeaturesPath] = useState("");
  const [modelPath, setModelPath] = useState("");
  const [params, setParams] = useState<TrainingParams>(DEFAULT_TRAINING_PARAMS);
  const [paramsSaved, setParamsSaved] = useState(false);
  const [includeDictionary, setIncludeDictionary] = useState(true);
  const [regenerateFeatures, setRegenerateFeatures] = useState(true);

  const [logs, setLogs] = useState<LogLine[]>([]);
  const [progress, setProgress] = useState<{ current: number; total: number } | null>(null);
  const [running, setRunning] = useState<RunningJob>(null);
  const [extractResult, setExtractResult] = useState<FeatureExtractionResult | null>(null);
  const [trainResult, setTrainResult] = useState<TrainingResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  const unlistenRef = useRef<UnlistenFn | null>(null);

  // Persisted hyperparameters, so a restart does not lose tuning work.
  useEffect(() => {
    loadTrainingParams()
      .then((stored) => setParams({ ...DEFAULT_TRAINING_PARAMS, ...stored }))
      .catch(() => {
        /* Not running inside Tauri: keep defaults. */
      });
  }, []);

  // Subscribe to the backend progress stream.
  useEffect(() => {
    let disposed = false;
    onProgress((event: ProgressEvent) => {
      setLogs((previous) => [
        ...previous,
        {
          timestamp: new Date().toLocaleTimeString(),
          jobId: event.jobId,
          stage: event.stage,
          message: event.message,
          level: event.level,
        },
      ]);
      if (event.current !== null && event.total !== null) {
        setProgress({ current: event.current, total: event.total });
      }
    })
      .then((unlisten) => {
        if (disposed) unlisten();
        else unlistenRef.current = unlisten;
      })
      .catch(() => {
        /* Progress events are unavailable outside the Tauri host. */
      });

    return () => {
      disposed = true;
      unlistenRef.current?.();
      unlistenRef.current = null;
    };
  }, []);

  const appendLocalLog = (stage: string, message: string, level: LogLine["level"]) => {
    setLogs((previous) => [
      ...previous,
      { timestamp: new Date().toLocaleTimeString(), jobId: "ui", stage, message, level },
    ]);
  };

  const addCorpus = async () => {
    try {
      const picked = await pickCorpusFile();
      if (!picked) return;
      setCorpusPaths((previous) =>
        previous.includes(picked) ? previous : [...previous, picked]
      );
      setExtractResult(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const browseFeatures = async () => {
    try {
      const picked = await pickCorpusFile();
      if (picked) setFeaturesPath(picked);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const browseModelOutput = async () => {
    try {
      const picked = await pickSaveFile(
        "Save trained CRF model",
        modelPath || env?.defaultModelPath || "bunsetsu_boundary.crfsuite",
        "crfsuite"
      );
      if (picked) setModelPath(picked);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const runExtraction = async () => {
    setRunning("extract");
    setError(null);
    setProgress(null);
    setExtractResult(null);
    appendLocalLog("extract", `Starting feature extraction for ${corpusPaths.length} file(s)`, "info");
    try {
      const result = await extractFeatures({
        corpusPaths,
        outputPath: featuresPath || null,
        includeDictionary,
        regenerateDictionaryFeatures: regenerateFeatures,
      });
      setExtractResult(result);
      if (result.outputPath && !featuresPath) setFeaturesPath(result.outputPath);
      appendLocalLog("extract", `Extraction finished: ${result.outputPath ?? "(no output)"}`, "success");
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      appendLocalLog("extract", message, "error");
    } finally {
      setRunning(null);
    }
  };

  const runTraining = async () => {
    setRunning("train");
    setError(null);
    setProgress(null);
    setTrainResult(null);
    appendLocalLog(
      "train",
      featuresPath
        ? `Training from pre-extracted features: ${featuresPath}`
        : `Training from ${corpusPaths.length} corpus file(s)`,
      "info"
    );
    try {
      const result = await trainModel({
        corpusPaths,
        featuresPath: featuresPath || null,
        modelPath: modelPath || null,
        params,
        includeDictionary,
        regenerateDictionaryFeatures: regenerateFeatures,
      });
      setTrainResult(result);
      if (result.modelPath) onModelsChanged(result.modelPath);
      onReloadModels();
      appendLocalLog(
        "train",
        result.success
          ? `Model written to ${result.modelPath}`
          : `Training failed: ${result.errorMessage ?? "unknown error"}`,
        result.success ? "success" : "error"
      );
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      appendLocalLog("train", message, "error");
    } finally {
      setRunning(null);
    }
  };

  const persistParams = async () => {
    try {
      await saveTrainingParams(params);
      setParamsSaved(true);
      window.setTimeout(() => setParamsSaved(false), 2000);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const canExtract = corpusPaths.length > 0 && running === null;
  const canTrain =
    running === null && (featuresPath.trim().length > 0 || corpusPaths.length > 0);

  const step1State = corpusPaths.length ? "done" : "pending";
  const step2State = running === "extract"
    ? "running"
    : extractResult
      ? "done"
      : corpusPaths.length
        ? "ready"
        : "pending";
  const step3State = running === "train"
    ? "running"
    : trainResult
      ? trainResult.success
        ? "done"
        : "error"
      : canTrain
        ? "ready"
        : "pending";

  const totalCorpusChars = useMemo(
    () => extractResult?.stats.totalChars ?? 0,
    [extractResult]
  );

  return (
    <div className="view">
      {env && !env.pycrfsuiteAvailable && (
        <p className="alert alert-warn">
          <strong>pycrfsuite was not found</strong>
          {env.pythonAvailable && env.pythonVersion ? ` (${env.pythonVersion})` : ""}. Training will
          fail until it is installed: <code>pip install python-crfsuite</code>
        </p>
      )}
      {error && <p className="alert alert-error">{error}</p>}

      {/* ── STEP 1: corpus ── */}
      <PipelineStep
        step={1}
        title="Training corpus"
        subtitle="Space-delimited bunsetsu, one sentence per line. Wrap passthrough segments in underscores."
        state={step1State}
        badge={<span className="badge">{corpusPaths.length} file(s)</span>}
        actions={
          <>
            <button type="button" className="btn btn-secondary btn-sm" onClick={addCorpus}>
              Add file…
            </button>
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              onClick={() => {
                setCorpusPaths([]);
                setExtractResult(null);
              }}
              disabled={!corpusPaths.length}
            >
              Clear
            </button>
          </>
        }
      >
        <p className="mono-note">
          Example: <code>きょう _は_ てんき _が_ よい</code> — hiragana, because the model
          predicts boundaries over yomi input.
        </p>

        {corpusPaths.length === 0 ? (
          <p className="empty-note">No corpus files selected.</p>
        ) : (
          <ul className="file-list">
            {corpusPaths.map((path, index) => (
              <li key={path}>
                <span className="file-path mono">{path}</span>
                <button
                  type="button"
                  className="btn btn-ghost btn-sm"
                  onClick={() => {
                    setCorpusPaths((previous) => previous.filter((_, i) => i !== index));
                    setExtractResult(null);
                  }}
                >
                  Remove
                </button>
              </li>
            ))}
          </ul>
        )}
      </PipelineStep>

      {/* ── STEP 2: feature extraction ── */}
      <PipelineStep
        step={2}
        title="Feature extraction"
        subtitle="Parse annotations and extract per-character CRF features, then dump them to a TSV."
        state={step2State}
        actions={
          <button
            type="button"
            className="btn btn-primary btn-sm"
            disabled={!canExtract}
            onClick={runExtraction}
          >
            {running === "extract" ? "Extracting…" : "Extract features"}
          </button>
        }
      >
        <div className="field-row">
          <label className="checkbox">
            <input
              type="checkbox"
              checked={regenerateFeatures}
              disabled={running !== null}
              onChange={(event) => setRegenerateFeatures(event.target.checked)}
            />
            Regenerate <code>crf_feature_materials.json</code> from the current dictionaries
          </label>
          <label className="checkbox">
            <input
              type="checkbox"
              checked={includeDictionary}
              disabled={running !== null}
              onChange={(event) => setIncludeDictionary(event.target.checked)}
            />
            Append extended-dictionary entries as one-bunsetsu training examples
          </label>
        </div>

        <div className="field">
          <label className="field-label" htmlFor="features-path">
            Features TSV output
          </label>
          <div className="field-row">
            <input
              id="features-path"
              className="input mono"
              type="text"
              value={featuresPath}
              placeholder={env?.defaultFeaturesPath ?? "crf_model_training_data.tsv"}
              onChange={(event) => setFeaturesPath(event.target.value)}
            />
            <button type="button" className="btn btn-secondary" onClick={browseFeatures}>
              Browse…
            </button>
          </div>
          <p className="field-hint">
            Two-step workflow: extract here, inspect the TSV in <strong>Corpus Stats</strong>,
            then train from it.
          </p>
        </div>

        {extractResult && (
          <div className="result-inline">
            <span className="badge badge-done">Extracted in {extractResult.elapsedSecs.toFixed(2)}s</span>
            <span>
              {extractResult.stats.sentenceCount.toLocaleString()} sentences ·{" "}
              {extractResult.stats.totalTokens.toLocaleString()} tokens ·{" "}
              {extractResult.stats.totalBunsetsu.toLocaleString()} bunsetsu (
              {extractResult.stats.lookupBunsetsu.toLocaleString()} lookup /{" "}
              {extractResult.stats.passthroughBunsetsu.toLocaleString()} passthrough)
            </span>
            {extractResult.dictEntries > 0 && (
              <span>
                + {extractResult.dictEntries.toLocaleString()} dictionary entries (
                {extractResult.dictTokens.toLocaleString()} tokens)
              </span>
            )}
            {extractResult.outputPath && (
              <span className="mono">{extractResult.outputPath}</span>
            )}
            {extractResult.isMock && <span className="badge badge-mock">MOCK</span>}
          </div>
        )}
        {totalCorpusChars > 0 && (
          <p className="field-hint">{totalCorpusChars.toLocaleString()} characters parsed.</p>
        )}
      </PipelineStep>

      {/* ── STEP 3: training ── */}
      <PipelineStep
        step={3}
        title="Train CRF model"
        subtitle="L-BFGS optimisation over the extracted features. Writes a .crfsuite model file."
        state={step3State}
        actions={
          <button
            type="button"
            className="btn btn-primary btn-sm"
            disabled={!canTrain}
            onClick={runTraining}
          >
            {running === "train" ? "Training…" : "Train"}
          </button>
        }
      >
        <div className="params-grid">
          <div className="field">
            <label className="field-label" htmlFor="algorithm">
              Algorithm
            </label>
            <select
              id="algorithm"
              className="input"
              value={params.algorithm}
              disabled={running !== null}
              onChange={(event) => setParams({ ...params, algorithm: event.target.value })}
            >
              <option value="lbfgs">lbfgs (L-BFGS)</option>
              <option value="l2sgd">l2sgd</option>
              <option value="ap">ap (averaged perceptron)</option>
              <option value="pa">pa (passive aggressive)</option>
              <option value="arow">arow</option>
            </select>
          </div>

          <div className="field">
            <label className="field-label" htmlFor="c1">
              c1 — L1 regularization
            </label>
            <input
              id="c1"
              className="input"
              type="number"
              step="0.1"
              min={0}
              value={params.c1}
              disabled={running !== null}
              onChange={(event) =>
                setParams({ ...params, c1: numberOr(event.target.value, params.c1) })
              }
            />
          </div>

          <div className="field">
            <label className="field-label" htmlFor="c2">
              c2 — L2 regularization
            </label>
            <input
              id="c2"
              className="input"
              type="number"
              step="0.001"
              min={0}
              value={params.c2}
              disabled={running !== null}
              onChange={(event) =>
                setParams({ ...params, c2: numberOr(event.target.value, params.c2) })
              }
            />
          </div>

          <div className="field">
            <label className="field-label" htmlFor="max-iter">
              max_iterations
            </label>
            <input
              id="max-iter"
              className="input"
              type="number"
              min={1}
              value={params.maxIterations}
              disabled={running !== null}
              onChange={(event) =>
                setParams({
                  ...params,
                  maxIterations: Math.max(1, Math.round(numberOr(event.target.value, 1))),
                })
              }
            />
          </div>

          <div className="field field-wide">
            <label className="checkbox">
              <input
                type="checkbox"
                checked={params.featurePossibleTransitions}
                disabled={running !== null}
                onChange={(event) =>
                  setParams({ ...params, featurePossibleTransitions: event.target.checked })
                }
              />
              feature.possible_transitions
            </label>
            <button type="button" className="btn btn-secondary btn-sm" onClick={persistParams}>
              {paramsSaved ? "Saved ✓" : "Save parameters"}
            </button>
          </div>
        </div>

        <div className="field">
          <label className="field-label" htmlFor="model-output">
            Model output
          </label>
          <div className="field-row">
            <input
              id="model-output"
              className="input mono"
              type="text"
              value={modelPath}
              placeholder={env?.defaultModelPath ?? "bunsetsu_boundary.crfsuite"}
              onChange={(event) => setModelPath(event.target.value)}
            />
            <button type="button" className="btn btn-secondary" onClick={browseModelOutput}>
              Browse…
            </button>
          </div>
        </div>

        {trainResult && (
          <div className="result-inline">
            <span className={`badge badge-${trainResult.success ? "done" : "error"}`}>
              {trainResult.success ? "Training complete" : "Training failed"}
            </span>
            <span>{trainResult.sentenceCount.toLocaleString()} sentences</span>
            <span>{trainResult.tokenCount.toLocaleString()} tokens</span>
            <span>{trainResult.trainingTimeSecs.toFixed(2)}s</span>
            {trainResult.lastIteration !== null && (
              <span>iteration {trainResult.lastIteration}</span>
            )}
            {trainResult.loss !== null && <span>loss {trainResult.loss}</span>}
            {trainResult.featureCount !== null && (
              <span>{trainResult.featureCount.toLocaleString()} features</span>
            )}
            <span>{formatBytes(trainResult.modelSizeBytes)}</span>
            {trainResult.isMock && <span className="badge badge-mock">MOCK</span>}
          </div>
        )}
      </PipelineStep>

      <LogConsole
        lines={logs}
        progress={progress}
        running={running !== null}
        onClear={() => {
          setLogs([]);
          setProgress(null);
        }}
      />

      <p className="muted footnote">
        After training, the produced model shows up in the <strong>Test</strong> tab model picker.
        Backend status for this scaffold: feature extraction and training currently emit{" "}
        <strong>mock</strong> results (no model file is written yet). Corpus parsing, statistics,
        model discovery and the parameter store are real.
      </p>
    </div>
  );
}
