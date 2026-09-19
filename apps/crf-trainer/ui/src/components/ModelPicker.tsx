import { formatBytes, formatEpochSeconds, pickModelFile } from "../api";
import type { ModelInfo } from "../types";

interface ModelPickerProps {
  models: ModelInfo[];
  value: string;
  onChange: (path: string) => void;
  onReload: () => void;
  /** Shown when the list is empty. */
  defaultModelPath: string;
  disabled?: boolean;
  label?: string;
}

/**
 * Model selector: discovered `.crfsuite` files plus a native browse fallback.
 * モデル選択: 検出された`.crfsuite`ファイルとネイティブ参照ダイアログ。
 */
export default function ModelPicker({
  models,
  value,
  onChange,
  onReload,
  defaultModelPath,
  disabled,
  label = "CRF model",
}: ModelPickerProps) {
  const selected = models.find((m) => m.path === value);

  const browse = async () => {
    const picked = await pickModelFile();
    if (picked) onChange(picked);
  };

  return (
    <div className="field">
      <label className="field-label" htmlFor="model-select">
        {label}
      </label>
      <div className="field-row">
        <select
          id="model-select"
          className="input"
          value={value}
          disabled={disabled}
          onChange={(event) => onChange(event.target.value)}
        >
          <option value="">
            {models.length ? "— select a model —" : "— no models found —"}
          </option>
          {models.map((model) => (
            <option key={model.path} value={model.path}>
              {model.fileName}
              {model.isDefault ? "  (default)" : ""} · {formatBytes(model.sizeBytes)}
            </option>
          ))}
          {/* Keep an externally-typed path visible in the dropdown. */}
          {value && !selected && <option value={value}>{value}</option>}
        </select>
        <button type="button" className="btn btn-secondary" onClick={browse} disabled={disabled}>
          Browse…
        </button>
        <button type="button" className="btn btn-ghost" onClick={onReload} disabled={disabled}>
          Rescan
        </button>
      </div>
      <p className="field-hint">
        {selected ? (
          <>
            {selected.path} · modified {formatEpochSeconds(selected.modified)}
          </>
        ) : (
          <>Default: {defaultModelPath}</>
        )}
      </p>
    </div>
  );
}
