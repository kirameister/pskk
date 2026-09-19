import type { ReactNode } from "react";

export type StepState = "pending" | "ready" | "running" | "done" | "error";

interface PipelineStepProps {
  step: number;
  title: string;
  subtitle: string;
  state: StepState;
  /** Rendered as a badge in the header, e.g. the corpus line count. */
  badge?: ReactNode;
  actions?: ReactNode;
  children: ReactNode;
}

const STATE_LABEL: Record<StepState, string> = {
  pending: "Pending",
  ready: "Ready",
  running: "Running…",
  done: "Done",
  error: "Error",
};

/**
 * One card of the browse → extract → train pipeline.
 * 参照 → 抽出 → 訓練パイプラインの1カード。
 */
export default function PipelineStep({
  step,
  title,
  subtitle,
  state,
  badge,
  actions,
  children,
}: PipelineStepProps) {
  return (
    <section className={`panel pipeline-step state-${state}`}>
      <header className="panel-head">
        <div className="step-heading">
          <span className="step-number">{step}</span>
          <div>
            <h3>{title}</h3>
            <p className="step-subtitle">{subtitle}</p>
          </div>
        </div>
        <div className="panel-head-actions">
          {badge}
          <span className={`badge badge-${state}`}>{STATE_LABEL[state]}</span>
          {actions}
        </div>
      </header>
      <div className="step-body">{children}</div>
    </section>
  );
}
