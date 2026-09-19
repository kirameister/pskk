import { useEffect, useMemo, useRef, useState } from "react";
import type { PredictionResult } from "../types";
import { FEATURE_LABELS } from "../types";

interface BoundaryCanvasProps {
  result: PredictionResult;
  candidateLabels: string[];
}

// ─── Geometry (CSS pixels; the canvas itself is scaled by devicePixelRatio) ──

const GUTTER = 128;
const BASE_TOKEN_W = 46;
const BASE_GAP_W = 26;
const HEADER_H = 34;
const BOUNDARY_H = 48;
const EMIT_H = 24;
const LABEL_H = 28;
const PAD = 10;

const SCALES = [0.8, 1, 1.25, 1.5] as const;

type Hover = { kind: "token"; index: number } | { kind: "gap"; index: number };

interface Layout {
  width: number;
  height: number;
  tokenW: number;
  gapW: number;
  tokenX: (index: number) => number;
  gapX: (index: number) => number;
}

function buildLayout(tokenCount: number, scale: number): Layout {
  const tokenW = Math.round(BASE_TOKEN_W * scale);
  const gapW = Math.round(BASE_GAP_W * scale);
  const gaps = Math.max(0, tokenCount - 1);
  const width = GUTTER + tokenCount * tokenW + gaps * gapW + PAD * 2;
  const emissionRows = 4; // upper bound used only for the height estimate
  const height =
    PAD * 2 + HEADER_H + BOUNDARY_H + emissionRows * EMIT_H + LABEL_H;

  return {
    width,
    height,
    tokenW,
    gapW,
    tokenX: (index: number) => PAD + GUTTER + index * (tokenW + gapW),
    gapX: (index: number) => PAD + GUTTER + index * (tokenW + gapW) + tokenW,
  };
}

// ─── Colour helpers ──────────────────────────────────────────────────

function mix(a: [number, number, number], b: [number, number, number], t: number): string {
  const r = Math.round(a[0] + (b[0] - a[0]) * t);
  const g = Math.round(a[1] + (b[1] - a[1]) * t);
  const bl = Math.round(a[2] + (b[2] - a[2]) * t);
  return `rgb(${r}, ${g}, ${bl})`;
}

const WHITE: [number, number, number] = [255, 255, 255];
const BLUE: [number, number, number] = [59, 130, 246];
const RED: [number, number, number] = [220, 38, 38];
const AMBER: [number, number, number] = [245, 158, 11];
const SLATE: [number, number, number] = [203, 213, 225];

/** Diverging scale for emission scores (blue = negative, red = positive). */
function emissionColor(value: number, maxAbs: number): string {
  const t = maxAbs > 0 ? Math.min(1, Math.abs(value) / maxAbs) : 0;
  const target = value >= 0 ? RED : BLUE;
  return mix(WHITE, target, t * 0.85);
}

/** Sequential scale for boundary confidence (slate = unlikely, red = certain). */
function boundaryColor(score: number): string {
  const t = Math.max(0, Math.min(1, score));
  return t < 0.5 ? mix(SLATE, AMBER, t * 2) : mix(AMBER, RED, (t - 0.5) * 2);
}

function truncate(text: string, max: number): string {
  return text.length <= max ? text : `${text.slice(0, Math.max(1, max - 1))}…`;
}

/**
 * Canvas rendering of one N-best candidate.
 *
 * 1つのN-best候補のキャンバス描画。
 *
 * A deliberate improvement over the GTK feature grid: instead of reading
 * numbers, you see where the model *wants* to cut. Each inter-token gap gets a
 * confidence bar, and emission scores become a heat strip per CRF label.
 *
 * GTKの特徴量グリッドに対する意図的な改善: 数値を読む代わりに、モデルが
 * どこで区切りたがっているかを可視化する。トークン間ギャップには信頼度バー、
 * emissionスコアはラベルごとのヒートストリップとして表示。
 */
export default function BoundaryCanvas({ result, candidateLabels }: BoundaryCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [hover, setHover] = useState<Hover | null>(null);
  const [scale, setScale] = useState<number>(1);

  const tokenCount = result.tokens.length;
  const labelCount = result.modelLabels.length;
  const boundaryScores = result.boundaryScores;

  const layout = useMemo(() => buildLayout(tokenCount, scale), [tokenCount, scale]);

  // Emission rows depend on the model, so the height estimate is fixed up here.
  const height = PAD * 2 + HEADER_H + BOUNDARY_H + labelCount * EMIT_H + LABEL_H;

  const maxEmission = useMemo(() => {
    let max = 0;
    for (const row of result.emissionScores) {
      for (const value of row) max = Math.max(max, Math.abs(value));
    }
    return max;
  }, [result.emissionScores]);

  const isBoundaryGap = (gapIndex: number): boolean =>
    (candidateLabels[gapIndex + 1] ?? "").startsWith("B");

  // ─── Draw ──────────────────────────────────────────────────────────
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.ceil(layout.width * dpr);
    canvas.height = Math.ceil(height * dpr);
    canvas.style.width = `${layout.width}px`;
    canvas.style.height = `${height}px`;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, layout.width, height);

    const headerY = PAD;
    const boundaryY = headerY + HEADER_H;
    const emissionY = boundaryY + BOUNDARY_H;
    const labelY = emissionY + labelCount * EMIT_H;

    // Gutters
    ctx.fillStyle = "#f8fafc";
    ctx.fillRect(0, 0, PAD + GUTTER, height);
    ctx.strokeStyle = "#e2e8f0";
    ctx.beginPath();
    ctx.moveTo(PAD + GUTTER, 0);
    ctx.lineTo(PAD + GUTTER, height);
    ctx.stroke();

    // Hover highlight (drawn first so text stays on top)
    if (hover) {
      const x =
        hover.kind === "token" ? layout.tokenX(hover.index) : layout.gapX(hover.index);
      const w = hover.kind === "token" ? layout.tokenW : layout.gapW;
      ctx.fillStyle = "rgba(37, 99, 235, 0.10)";
      ctx.fillRect(x, headerY, w, height - headerY - PAD);
    }

    // Predicted boundary guide lines, spanning every row
    ctx.save();
    ctx.strokeStyle = "rgba(220, 38, 38, 0.45)";
    ctx.lineWidth = 2;
    for (let gap = 0; gap < tokenCount - 1; gap += 1) {
      if (!isBoundaryGap(gap)) continue;
      const x = layout.gapX(gap) + layout.gapW / 2;
      ctx.beginPath();
      ctx.moveTo(x, headerY + HEADER_H - 6);
      ctx.lineTo(x, labelY + LABEL_H);
      ctx.stroke();
    }
    ctx.restore();

    // ── Row 1: token / gap header ──
    ctx.textBaseline = "middle";
    ctx.textAlign = "center";
    for (let i = 0; i < tokenCount; i += 1) {
      const x = layout.tokenX(i);
      ctx.fillStyle = "#f1f5f9";
      ctx.fillRect(x, headerY, layout.tokenW, HEADER_H);
      ctx.strokeStyle = "#e2e8f0";
      ctx.strokeRect(x + 0.5, headerY + 0.5, layout.tokenW - 1, HEADER_H - 1);

      ctx.fillStyle = "#0f172a";
      ctx.font = `600 ${Math.round(14 * scale)}px "Noto Sans JP", system-ui, sans-serif`;
      ctx.fillText(result.tokens[i], x + layout.tokenW / 2, headerY + HEADER_H / 2);
    }
    ctx.fillStyle = "#94a3b8";
    ctx.font = `12px system-ui, sans-serif`;
    for (let gap = 0; gap < tokenCount - 1; gap += 1) {
      ctx.fillText("→", layout.gapX(gap) + layout.gapW / 2, headerY + HEADER_H / 2);
    }

    // ── Row 2: boundary confidence ──
    ctx.fillStyle = "#0f172a";
    ctx.font = "600 11px system-ui, sans-serif";
    ctx.textAlign = "right";
    ctx.fillText("boundary P(split)", PAD + GUTTER - 10, boundaryY + BOUNDARY_H / 2);
    ctx.textAlign = "center";

    for (let gap = 0; gap < tokenCount - 1; gap += 1) {
      const x = layout.gapX(gap);
      const score = boundaryScores[gap] ?? 0;
      const barW = Math.max(6, layout.gapW - 10);
      const barH = Math.max(2, (BOUNDARY_H - 18) * Math.max(0, Math.min(1, score)));

      ctx.fillStyle = "#f1f5f9";
      ctx.fillRect(x + (layout.gapW - barW) / 2, boundaryY + 4, barW, BOUNDARY_H - 8);
      ctx.fillStyle = boundaryColor(score);
      ctx.fillRect(
        x + (layout.gapW - barW) / 2,
        boundaryY + BOUNDARY_H - 4 - barH,
        barW,
        barH
      );

      ctx.fillStyle = score >= 0.5 ? "#7f1d1d" : "#475569";
      ctx.font = `600 ${Math.round(10 * scale)}px ui-monospace, monospace`;
      ctx.fillText(score.toFixed(2), x + layout.gapW / 2, boundaryY + 12);

      if (isBoundaryGap(gap)) {
        ctx.fillStyle = "#dc2626";
        ctx.font = `700 ${Math.round(11 * scale)}px system-ui, sans-serif`;
        ctx.fillText("B", x + layout.gapW / 2, boundaryY + BOUNDARY_H - 10);
      }
    }

    // ── Row 3..: emission heat per CRF label ──
    result.modelLabels.forEach((label, labelIndex) => {
      const y = emissionY + labelIndex * EMIT_H;

      ctx.textAlign = "right";
      ctx.fillStyle = label.endsWith("-L") ? "#1d4ed8" : "#7c3aed";
      ctx.font = "600 11px ui-monospace, monospace";
      ctx.fillText(`emit ${label}`, PAD + GUTTER - 10, y + EMIT_H / 2);
      ctx.textAlign = "center";

      for (let i = 0; i < tokenCount; i += 1) {
        const x = layout.tokenX(i);
        const value = result.emissionScores[i]?.[labelIndex] ?? 0;
        ctx.fillStyle = emissionColor(value, maxEmission);
        ctx.fillRect(x, y, layout.tokenW, EMIT_H);
        ctx.strokeStyle = "#eef2f7";
        ctx.strokeRect(x + 0.5, y + 0.5, layout.tokenW - 1, EMIT_H - 1);

        if (layout.tokenW >= 34) {
          ctx.fillStyle = Math.abs(value) > maxEmission * 0.6 ? "#ffffff" : "#334155";
          ctx.font = `10px ui-monospace, monospace`;
          ctx.fillText(truncate(value.toFixed(2), 5), x + layout.tokenW / 2, y + EMIT_H / 2);
        }
      }
    });

    // ── Last row: predicted label per token ──
    ctx.textAlign = "right";
    ctx.fillStyle = "#0f172a";
    ctx.font = "600 11px system-ui, sans-serif";
    ctx.fillText("label", PAD + GUTTER - 10, labelY + LABEL_H / 2);
    ctx.textAlign = "center";

    for (let i = 0; i < tokenCount; i += 1) {
      const x = layout.tokenX(i);
      const label = candidateLabels[i] ?? "—";
      const isLookup = label.endsWith("-L");
      ctx.fillStyle = label.startsWith("B") ? (isLookup ? "#1d4ed8" : "#7c3aed") : "#64748b";
      ctx.fillRect(x, labelY, layout.tokenW, LABEL_H);

      ctx.fillStyle = "#ffffff";
      ctx.font = `700 ${Math.round(11 * scale)}px ui-monospace, monospace`;
      ctx.fillText(truncate(label, 4), x + layout.tokenW / 2, labelY + LABEL_H / 2);
    }
  }, [
    layout,
    height,
    result,
    candidateLabels,
    hover,
    maxEmission,
    tokenCount,
    labelCount,
    scale,
  ]);

  // ─── Pointer → cell mapping ────────────────────────────────────────
  const handleMove = (event: React.MouseEvent<HTMLCanvasElement>) => {
    const x = event.nativeEvent.offsetX;
    if (x < PAD + GUTTER) {
      setHover(null);
      return;
    }
    const rel = x - (PAD + GUTTER);
    const period = layout.tokenW + layout.gapW;
    const index = Math.floor(rel / period);
    if (index >= tokenCount) {
      setHover(null);
      return;
    }
    const within = rel - index * period;
    if (within <= layout.tokenW || index >= tokenCount - 1) {
      setHover({ kind: "token", index });
    } else {
      setHover({ kind: "gap", index });
    }
  };

  // ─── Readout ───────────────────────────────────────────────────────
  const readout = (() => {
    if (!hover) {
      return <p className="empty-note">Hover a token or a gap to inspect its scores.</p>;
    }
    if (hover.kind === "token") {
      const index = hover.index;
      const features = result.features[index] ?? [];
      return (
        <div className="canvas-readout">
          <div className="readout-title">
            token #{index} <strong>{result.tokens[index]}</strong> · predicted{" "}
            <strong>{candidateLabels[index]}</strong>
          </div>
          <div className="readout-grid">
            <div>
              <span className="readout-label">emissions</span>
              {result.modelLabels.map((label, labelIndex) => (
                <span key={label} className="readout-item">
                  {label} {(result.emissionScores[index]?.[labelIndex] ?? 0).toFixed(2)}
                </span>
              ))}
            </div>
            <div>
              <span className="readout-label">features</span>
              {features.map((feature) => (
                <span key={feature.key} className="readout-item">
                  {FEATURE_LABELS[feature.key] ?? feature.key}={feature.value}
                </span>
              ))}
            </div>
          </div>
        </div>
      );
    }
    const gap = hover.index;
    const score = boundaryScores[gap] ?? 0;
    const from = candidateLabels[gap] ?? "—";
    const to = candidateLabels[gap + 1] ?? "—";
    const transitions = result.transitions.filter((t) => t.from === from);
    return (
      <div className="canvas-readout">
        <div className="readout-title">
          gap {gap}→{gap + 1} · P(split) <strong>{score.toFixed(3)}</strong> ·{" "}
          {isBoundaryGap(gap) ? (
            <span className="tag tag-split">boundary</span>
          ) : (
            <span className="tag">inside bunsetsu</span>
          )}
        </div>
        <div className="readout-grid">
          <div>
            <span className="readout-label">transition</span>
            <span className="readout-item">
              {from} → {to}
            </span>
          </div>
          <div>
            <span className="readout-label">scores from {from}</span>
            {transitions.map((t) => (
              <span key={t.to} className="readout-item">
                →{t.to} {t.score.toFixed(2)}
              </span>
            ))}
          </div>
        </div>
      </div>
    );
  })();

  return (
    <div className="canvas-wrap">
      <div className="canvas-toolbar">
        <span className="muted">Zoom</span>
        {SCALES.map((value) => (
          <button
            key={value}
            type="button"
            className={`btn btn-sm ${scale === value ? "btn-primary" : "btn-ghost"}`}
            onClick={() => setScale(value)}
          >
            {Math.round(value * 100)}%
          </button>
        ))}
        <span className="legend">
          <span className="legend-swatch" style={{ background: emissionColor(maxEmission, maxEmission) }} />
          high emission
          <span className="legend-swatch" style={{ background: emissionColor(-maxEmission, maxEmission) }} />
          low emission
          <span className="legend-swatch" style={{ background: boundaryColor(0.95) }} />
          strong boundary
          <span className="legend-swatch" style={{ background: boundaryColor(0.05) }} />
          weak boundary
        </span>
      </div>

      <div className="canvas-scroll">
        <canvas
          ref={canvasRef}
          onMouseMove={handleMove}
          onMouseLeave={() => setHover(null)}
          style={{ cursor: hover?.kind === "gap" ? "col-resize" : "default" }}
        />
      </div>

      {readout}
    </div>
  );
}
