import { useMemo } from "react";
import type { PredictionResult, ResultViewMode } from "../types";
import BoundaryCanvas from "./BoundaryCanvas";
import BunsetsuPreview from "./BunsetsuPreview";
import FeatureGrid from "./FeatureGrid";

interface NBestPanelProps {
  result: PredictionResult;
  mode: ResultViewMode;
  onModeChange: (mode: ResultViewMode) => void;
  activeIndex: number;
  onActiveIndexChange: (index: number) => void;
}

/**
 * N-best result browser: one tab per candidate, each with a bunsetsu preview
 * and either the numeric feature grid or the canvas visualisation.
 *
 * N-best結果ブラウザ: 候補ごとのタブ、文節プレビュー、数値グリッドまたは
 * キャンバス可視化。
 */
export default function NBestPanel({
  result,
  mode,
  onModeChange,
  activeIndex,
  onActiveIndexChange,
}: NBestPanelProps) {
  const candidates = result.candidates;
  const active = candidates[Math.min(activeIndex, candidates.length - 1)];

  const splitText = useMemo(() => {
    if (!active) return "";
    return active.bunsetsu
      .map((item) => (item.isLookup ? item.text : `_${item.text}_`))
      .join(" ");
  }, [active]);

  if (!candidates.length) {
    return (
      <section className="panel">
        <p className="empty-note">The model returned no candidates.</p>
      </section>
    );
  }

  return (
    <section className="panel result-panel">
      <header className="panel-head">
        <div className="tabs" role="tablist" aria-label="N-best candidates">
          {candidates.map((candidate, index) => (
            <button
              key={candidate.rank}
              type="button"
              role="tab"
              aria-selected={index === activeIndex}
              className={`tab${index === activeIndex ? " active" : ""}`}
              onClick={() => onActiveIndexChange(index)}
            >
              #{candidate.rank}
              <span className="tab-score">{candidate.score.toFixed(3)}</span>
            </button>
          ))}
        </div>
        <div className="panel-head-actions">
          <div className="segmented">
            <button
              type="button"
              className={`segmented-btn${mode === "table" ? " active" : ""}`}
              onClick={() => onModeChange("table")}
            >
              Table
            </button>
            <button
              type="button"
              className={`segmented-btn${mode === "canvas" ? " active" : ""}`}
              onClick={() => onModeChange("canvas")}
            >
              Canvas
            </button>
          </div>
        </div>
      </header>

      <div className="bunsetsu-bar">
        <BunsetsuPreview bunsetsu={active.bunsetsu} size="large" />
        <code className="split-text" title="Annotated form: passthrough bunsetsu are wrapped in underscores">
          {splitText}
        </code>
      </div>

      {mode === "table" ? (
        <FeatureGrid result={result} candidateLabels={active.labels} />
      ) : (
        <BoundaryCanvas result={result} candidateLabels={active.labels} />
      )}
    </section>
  );
}
