import type { PredictionResult } from "../types";
import { DISPLAY_FEATURE_KEYS, FEATURE_LABELS } from "../types";

interface FeatureGridProps {
  result: PredictionResult;
  /** The N-best candidate whose labels drive the `Label` row. */
  candidateLabels: string[];
}

type Column =
  | { kind: "token"; tokenIndex: number }
  | { kind: "gap"; gapIndex: number };

interface TransitionRow {
  from: string;
  to: string;
}

function scoreClass(value: number): string {
  if (value > 0.05) return "score-pos";
  if (value < -0.05) return "score-neg";
  return "score-zero";
}

const fmt = (value: number | null | undefined): string =>
  value === null || value === undefined || !Number.isFinite(value) ? "—" : value.toFixed(2);

function buildColumns(tokenCount: number): Column[] {
  const columns: Column[] = [];
  for (let i = 0; i < tokenCount; i += 1) {
    columns.push({ kind: "token", tokenIndex: i });
    if (i < tokenCount - 1) columns.push({ kind: "gap", gapIndex: i });
  }
  return columns;
}

/**
 * The per-token score grid from the GTK panel: predicted label, transition
 * scores, emission scores and feature values, with tokens and inter-token
 * gaps interleaved as columns.
 *
 * GTKパネルと同じトークン単位のスコアグリッド。予測ラベル、遷移スコア、
 *  emissionスコア、特徴量を、トークンとトークン間ギャップを交互に並べて表示。
 */
export default function FeatureGrid({ result, candidateLabels }: FeatureGridProps) {
  const columns = buildColumns(result.tokens.length);
  const labelCount = result.modelLabels.length;

  // Row headers for the transition group. Debug mode shows every M×M pair;
  // the default view shows only transitions out of each candidate label.
  const transitionRows: TransitionRow[] = result.debug
    ? result.modelLabels.flatMap((from) => result.modelLabels.map((to) => ({ from, to })))
    : result.modelLabels.map((to) => ({ from: "", to }));

  const transitionScore = (from: string, to: string): number | null => {
    const hit = result.transitions.find((t) => t.from === from && t.to === to);
    return hit ? hit.score : null;
  };

  const featureWeight = (tokenIndex: number, key: string): number | null => {
    const feature = result.features[tokenIndex]?.find((f) => f.key === key);
    return feature ? feature.weight : null;
  };

  const featureValue = (tokenIndex: number, key: string): string => {
    const feature = result.features[tokenIndex]?.find((f) => f.key === key);
    return feature ? feature.value : "—";
  };

  const isBoundaryGap = (gapIndex: number): boolean =>
    (candidateLabels[gapIndex + 1] ?? "").startsWith("B");

  const columnCount = columns.length + 1;

  return (
    <div className="grid-scroll">
      <table className="feature-grid">
        <thead>
          <tr>
            <th className="row-head sticky-col">row \ token</th>
            {columns.map((column, index) =>
              column.kind === "token" ? (
                <th key={index} className="col-head token-col">
                  <span className="token-char">{result.tokens[column.tokenIndex]}</span>
                  <span className="token-index">{column.tokenIndex}</span>
                </th>
              ) : (
                <th
                  key={index}
                  className={`col-head gap-col${isBoundaryGap(column.gapIndex) ? " boundary" : ""}`}
                  title={isBoundaryGap(column.gapIndex) ? "Predicted bunsetsu boundary" : "Inside bunsetsu"}
                >
                  →
                </th>
              )
            )}
          </tr>
        </thead>
        <tbody>
          {/* ── Predicted labels ── */}
          <tr className="group-row">
            <th className="group-head sticky-col" colSpan={columnCount}>
              Prediction
            </th>
          </tr>
          <tr>
            <th className="row-head sticky-col">Label</th>
            {columns.map((column, index) => {
              if (column.kind !== "token") {
                return <td key={index} className="gap-col" />;
              }
              const label = candidateLabels[column.tokenIndex] ?? "—";
              return (
                <td key={index} className={`token-col label-cell kind-${label.slice(-1)}`}>
                  {label}
                </td>
              );
            })}
          </tr>

          {/* ── Transitions ── */}
          <tr className="group-row">
            <th className="group-head sticky-col" colSpan={columnCount}>
              Transitions{result.debug ? " (all M×M)" : " (from previous label)"}
            </th>
          </tr>
          {!result.debug && (
            <tr className="sub-row">
              <th className="row-head sticky-col">from</th>
              {columns.map((column, index) =>
                column.kind === "gap" ? (
                  <td key={index} className={`gap-col kind-${(candidateLabels[column.gapIndex] ?? "L").slice(-1)}`}>
                    {candidateLabels[column.gapIndex] ?? "—"}
                  </td>
                ) : (
                  <td key={index} className="token-col" />
                )
              )}
            </tr>
          )}
          {transitionRows.map((row) => (
            <tr key={`${row.from}-${row.to}`}>
              <th className="row-head sticky-col">
                {result.debug ? `tr ${row.from}→${row.to}` : `tr prev→${row.to}`}
              </th>
              {columns.map((column, index) => {
                if (column.kind !== "gap") {
                  return <td key={index} className="token-col" />;
                }
                const from = result.debug
                  ? row.from
                  : candidateLabels[column.gapIndex] ?? "";
                const score = transitionScore(from, row.to);
                return (
                  <td key={index} className={`gap-col num ${scoreClass(score ?? 0)}`}>
                    {fmt(score)}
                  </td>
                );
              })}
            </tr>
          ))}

          {/* ── Emissions ── */}
          <tr className="group-row">
            <th className="group-head sticky-col" colSpan={columnCount}>
              Emission scores
            </th>
          </tr>
          {result.modelLabels.map((label, labelIndex) => (
            <tr key={label} className={`emit-row kind-${label.slice(-1)}`}>
              <th className="row-head sticky-col">{`emit ${label}`}</th>
              {columns.map((column, index) => {
                if (column.kind !== "token") {
                  return <td key={index} className="gap-col" />;
                }
                const score = result.emissionScores[column.tokenIndex]?.[labelIndex];
                return (
                  <td key={index} className={`token-col num ${scoreClass(score ?? 0)}`}>
                    {fmt(score)}
                  </td>
                );
              })}
            </tr>
          ))}

          {/* ── Features ── */}
          <tr className="group-row">
            <th className="group-head sticky-col" colSpan={columnCount}>
              Features
            </th>
          </tr>
          {DISPLAY_FEATURE_KEYS.map((key) => (
            <tr key={key}>
              <th className="row-head sticky-col" title={key}>
                {FEATURE_LABELS[key] ?? key}
              </th>
              {columns.map((column, index) =>
                column.kind === "token" ? (
                  <td key={index} className="token-col feat-cell">
                    {featureValue(column.tokenIndex, key)}
                  </td>
                ) : (
                  <td key={index} className="gap-col" />
                )
              )}
            </tr>
          ))}

          {/* ── Feature weights (debug only) ── */}
          {result.debug && (
            <>
              <tr className="group-row">
                <th className="group-head sticky-col" colSpan={columnCount}>
                  Feature weights for the predicted label
                </th>
              </tr>
              {DISPLAY_FEATURE_KEYS.map((key) => (
                <tr key={`w-${key}`}>
                  <th className="row-head sticky-col">{`${FEATURE_LABELS[key] ?? key} · w`}</th>
                  {columns.map((column, index) => {
                    if (column.kind !== "token") {
                      return <td key={index} className="gap-col" />;
                    }
                    const weight = featureWeight(column.tokenIndex, key);
                    return (
                      <td key={index} className={`token-col num ${scoreClass(weight ?? 0)}`}>
                        {fmt(weight)}
                      </td>
                    );
                  })}
                </tr>
              ))}
            </>
          )}
        </tbody>
      </table>
      {labelCount === 0 && <p className="empty-note">The model exposes no labels.</p>}
    </div>
  );
}
