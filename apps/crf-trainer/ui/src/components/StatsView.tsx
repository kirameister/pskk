import { useState } from "react";
import { formatBytes, inspectFeatureTsv, loadCorpusReport, pickCorpusFile } from "../api";
import type {
  CorpusReport,
  EnvironmentInfo,
  FeatureTsvReport,
  SampleSentence,
} from "../types";
import BunsetsuPreview from "./BunsetsuPreview";

interface StatsViewProps {
  env: EnvironmentInfo | null;
}

function StatTile({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <div className="summary-card">
      <span className="summary-label">{label}</span>
      <span className="summary-value">{value}</span>
      {hint && <span className="summary-hint">{hint}</span>}
    </div>
  );
}

function SentenceRow({ sentence }: { sentence: SampleSentence }) {
  return (
    <tr>
      <td className="mono dim">{sentence.lineNumber}</td>
      <td>
        <BunsetsuPreview bunsetsu={sentence.bunsetsu} size="inline" />
      </td>
      <td className="mono small">{sentence.raw}</td>
    </tr>
  );
}

/**
 * Corpus Stats tab: statistics for annotated corpora plus an inspector for the
 * intermediate features TSV produced by the extraction step.
 *
 * コーパス統計タブ: 注釈付きコーパスの統計と、抽出ステップが生成する
 * 中間特徴量TSVのインスペクタ。
 */
export default function StatsView({ env }: StatsViewProps) {
  const [paths, setPaths] = useState<string[]>([]);
  const [sampleLimit, setSampleLimit] = useState(20);
  const [report, setReport] = useState<CorpusReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [tsvPath, setTsvPath] = useState("");
  const [tsvReport, setTsvReport] = useState<FeatureTsvReport | null>(null);
  const [tsvLoading, setTsvLoading] = useState(false);
  const [tsvError, setTsvError] = useState<string | null>(null);

  const addFile = async () => {
    try {
      const picked = await pickCorpusFile();
      if (!picked) return;
      setPaths((previous) => (previous.includes(picked) ? previous : [...previous, picked]));
      setReport(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const compute = async () => {
    if (!paths.length) return;
    setLoading(true);
    setError(null);
    try {
      setReport(await loadCorpusReport(paths, sampleLimit));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setReport(null);
    } finally {
      setLoading(false);
    }
  };

  const inspect = async () => {
    if (!tsvPath.trim()) return;
    setTsvLoading(true);
    setTsvError(null);
    try {
      setTsvReport(await inspectFeatureTsv(tsvPath, 25));
    } catch (err) {
      setTsvError(err instanceof Error ? err.message : String(err));
      setTsvReport(null);
    } finally {
      setTsvLoading(false);
    }
  };

  const combined = report?.combined;

  return (
    <div className="view">
      {/* ── Corpus statistics ── */}
      <section className="panel">
        <header className="panel-head">
          <h2>Corpus statistics</h2>
          <div className="panel-head-actions">
            <label className="inline-field">
              samples
              <input
                className="input input-xs"
                type="number"
                min={0}
                max={200}
                value={sampleLimit}
                onChange={(event) =>
                  setSampleLimit(Math.max(0, Math.min(200, Number(event.target.value) || 0)))
                }
              />
            </label>
            <button type="button" className="btn btn-secondary btn-sm" onClick={addFile}>
              Add file…
            </button>
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              disabled={!paths.length}
              onClick={() => {
                setPaths([]);
                setReport(null);
              }}
            >
              Clear
            </button>
            <button
              type="button"
              className="btn btn-primary btn-sm"
              disabled={!paths.length || loading}
              onClick={compute}
            >
              {loading ? "Computing…" : "Compute statistics"}
            </button>
          </div>
        </header>

        {paths.length === 0 ? (
          <p className="empty-note">
            Add one or more annotated corpus files. Files are combined, and per-file statistics are
            reported separately.
          </p>
        ) : (
          <ul className="file-list">
            {paths.map((path, index) => (
              <li key={path}>
                <span className="file-path mono">{path}</span>
                <button
                  type="button"
                  className="btn btn-ghost btn-sm"
                  onClick={() => {
                    setPaths((previous) => previous.filter((_, i) => i !== index));
                    setReport(null);
                  }}
                >
                  Remove
                </button>
              </li>
            ))}
          </ul>
        )}

        {error && <p className="alert alert-error">{error}</p>}

        {report && report.warnings.length > 0 && (
          <ul className="alert alert-warn warning-list">
            {report.warnings.map((warning) => (
              <li key={warning}>{warning}</li>
            ))}
          </ul>
        )}

        {combined && (
          <>
            <div className="summary-row">
              <StatTile label="Files" value={String(report.perFile.length)} />
              <StatTile label="Sentences" value={combined.sentenceCount.toLocaleString()} />
              <StatTile label="Tokens" value={combined.totalTokens.toLocaleString()} />
              <StatTile label="Bunsetsu" value={combined.totalBunsetsu.toLocaleString()} />
              <StatTile
                label="Lookup / passthrough"
                value={`${combined.lookupBunsetsu.toLocaleString()} / ${combined.passthroughBunsetsu.toLocaleString()}`}
                hint={combined.totalBunsetsu
                  ? `${((combined.lookupBunsetsu / combined.totalBunsetsu) * 100).toFixed(1)}% lookup`
                  : undefined}
              />
              <StatTile label="Characters" value={combined.totalChars.toLocaleString()} />
              <StatTile
                label="Avg tokens / sentence"
                value={
                  combined.sentenceCount
                    ? (combined.totalTokens / combined.sentenceCount).toFixed(2)
                    : "—"
                }
              />
              <StatTile
                label="Avg bunsetsu / sentence"
                value={
                  combined.sentenceCount
                    ? (combined.totalBunsetsu / combined.sentenceCount).toFixed(2)
                    : "—"
                }
              />
            </div>

            <div className="grid-scroll">
              <table className="data-table">
                <thead>
                  <tr>
                    <th>File</th>
                    <th>Lines</th>
                    <th>Sentences</th>
                    <th>Tokens</th>
                    <th>Bunsetsu</th>
                    <th>Lookup</th>
                    <th>Passthrough</th>
                    <th>Chars</th>
                  </tr>
                </thead>
                <tbody>
                  {report.perFile.map((stats) => (
                    <tr key={stats.source}>
                      <td className="mono small">{stats.source}</td>
                      <td className="num">{stats.lineCount.toLocaleString()}</td>
                      <td className="num">{stats.sentenceCount.toLocaleString()}</td>
                      <td className="num">{stats.totalTokens.toLocaleString()}</td>
                      <td className="num">{stats.totalBunsetsu.toLocaleString()}</td>
                      <td className="num">{stats.lookupBunsetsu.toLocaleString()}</td>
                      <td className="num">{stats.passthroughBunsetsu.toLocaleString()}</td>
                      <td className="num">{stats.totalChars.toLocaleString()}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </>
        )}

        {report && report.samples.length > 0 && (
          <div className="subsection">
            <h3>
              Sample sentences{" "}
              <span className="muted">
                ({report.samples.length} shown{report.samplesTruncated ? ", truncated" : ""})
              </span>
            </h3>
            <div className="grid-scroll">
              <table className="data-table">
                <thead>
                  <tr>
                    <th>Line</th>
                    <th>Bunsetsu split (bold = lookup)</th>
                    <th>Annotated source</th>
                  </tr>
                </thead>
                <tbody>
                  {report.samples.map((sentence) => (
                    <SentenceRow key={`${sentence.lineNumber}-${sentence.raw}`} sentence={sentence} />
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        )}
      </section>

      {/* ── Features TSV inspector ── */}
      <section className="panel">
        <header className="panel-head">
          <h2>Features TSV inspector</h2>
          <div className="panel-head-actions">
            <button
              type="button"
              className="btn btn-primary btn-sm"
              disabled={!tsvPath.trim() || tsvLoading}
              onClick={inspect}
            >
              {tsvLoading ? "Inspecting…" : "Inspect"}
            </button>
          </div>
        </header>

        <div className="field">
          <label className="field-label" htmlFor="tsv-path">
            Extracted features file
          </label>
          <div className="field-row">
            <input
              id="tsv-path"
              className="input mono"
              type="text"
              value={tsvPath}
              placeholder={env?.defaultFeaturesPath ?? "crf_model_training_data.tsv"}
              onChange={(event) => setTsvPath(event.target.value)}
            />
            <button
              type="button"
              className="btn btn-secondary"
              onClick={async () => {
                const picked = await pickCorpusFile();
                if (picked) setTsvPath(picked);
              }}
            >
              Browse…
            </button>
          </div>
          <p className="field-hint">
            Format written by feature extraction: <code>token ⇥ label ⇥ key=value …</code>, with{" "}
            <code># Sentence N</code> markers between sentences.
          </p>
        </div>

        {tsvError && <p className="alert alert-error">{tsvError}</p>}

        {tsvReport && (
          <>
            <div className="summary-row">
              <StatTile label="Size" value={formatBytes(tsvReport.sizeBytes)} />
              <StatTile label="Sentences" value={tsvReport.sentenceCount.toLocaleString()} />
              <StatTile label="Tokens" value={tsvReport.tokenCount.toLocaleString()} />
              <StatTile label="Feature keys" value={String(tsvReport.featureKeys.length)} />
              {tsvReport.labelCounts.map((entry) => (
                <StatTile
                  key={entry.label}
                  label={entry.label}
                  value={entry.count.toLocaleString()}
                />
              ))}
            </div>

            {tsvReport.warnings.length > 0 && (
              <ul className="alert alert-warn warning-list">
                {tsvReport.warnings.map((warning) => (
                  <li key={warning}>{warning}</li>
                ))}
              </ul>
            )}

            <div className="subsection">
              <h3>Feature keys</h3>
              <div className="chip-row">
                {tsvReport.featureKeys.map((key) => (
                  <span key={key} className="chip chip-static">
                    {key}
                  </span>
                ))}
              </div>
            </div>

            <div className="subsection">
              <h3>
                Sentences{" "}
                <span className="muted">
                  ({tsvReport.sentences.length} shown{tsvReport.truncated ? ", truncated" : ""})
                </span>
              </h3>
              <div className="grid-scroll">
                <table className="data-table">
                  <thead>
                    <tr>
                      <th>#</th>
                      <th>Line</th>
                      <th>Tokens</th>
                      <th>Bunsetsu split</th>
                      <th>Labels</th>
                    </tr>
                  </thead>
                  <tbody>
                    {tsvReport.sentences.map((sentence) => (
                      <tr key={sentence.index}>
                        <td className="num dim">{sentence.index}</td>
                        <td className="num dim">{sentence.lineNumber}</td>
                        <td className="num">{sentence.tokens.length}</td>
                        <td>
                          <BunsetsuPreview bunsetsu={sentence.bunsetsu} size="inline" />
                        </td>
                        <td className="mono small">{sentence.labels.join(" ")}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          </>
        )}
      </section>
    </div>
  );
}
