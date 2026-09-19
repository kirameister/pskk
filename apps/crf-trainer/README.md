# PSKK CRF Trainer

Tauri desktop app for **training and testing the CRF bunsetsu-segmentation model**
used by the PSKK IME — a GUI-first reimagining of the GTK panel in
`conversion_model.py`, with a canvas visualisation of predictions that the Python
implementation never had.

PSKK IMEのCRF文節分割モデルを**訓練・テスト**するためのTauriデスクトップアプリ。
`conversion_model.py`のGTKパネルをGUIファーストで再設計し、Python実装には
なかった予測のキャンバス可視化を追加したもの。

---

## Status / 実装状況

This first pass delivers **the complete GUI plus the real non-CRF plumbing**.
The CRF-dependent steps are mocked so the UI (including the canvas) can be
designed and reviewed before the Python bridge exists.

| Area | Command / feature | Status |
| --- | --- | --- |
| Environment probe (`python`, `pycrfsuite`) | `get_environment` | **Real** |
| Model discovery (`.crfsuite` scan) | `list_models` | **Real** |
| Native file dialogs | `pick_corpus_file`, `pick_model_file`, `pick_save_file` | **Real** |
| Corpus parsing + statistics | `load_corpus_report` | **Real** |
| Features-TSV inspector | `inspect_feature_tsv` | **Real** |
| Hyperparameter persistence | `load_training_params`, `save_training_params` | **Real** |
| Feature extraction | `extract_features` | **MOCK** |
| CRF training | `train_model` | **MOCK** |
| Prediction (N-best) | `predict` | **MOCK** |

MOCK commands return well-formed synthetic payloads tagged `isMock: true`; the UI
shows a purple `MOCK` badge wherever such data appears. Nothing is written to the
model or features paths yet.

モックコマンドは`isMock: true`を持つ合成データを返し、UIは紫色の`MOCK`バッジを
表示する。モデルや特徴量ファイルはまだ書き込まれない。

---

## Running / 実行

```bash
# From the repository root
just crf-trainer-ui-install   # one-off: npm install
just dev-crf-trainer          # cargo tauri dev

# Build a release binary + packages (deb/rpm)
just crf-trainer-build
just crf-trainer-install      # installs /opt/pskk/bin/pskk-crf-trainer
```

The Vite dev server binds `127.0.0.1:1422` (the other apps use 1420/1421).
Opening that URL in a plain browser does not work: the UI needs the Tauri host
for `invoke`, and reports a readable error if it is missing.

Vite開発サーバーは`127.0.0.1:1422`を使用する（他のアプリは1420/1421）。
Tauriホストなしでブラウザで開いても動作しない。

---

## GUI layout / 画面構成

Three views, selectable from the left sidebar.

### 1. Test

- Hiragana input with one-click examples, N-best count (1–10) and a
  **Debug detail** toggle that requests all M×M transitions plus feature weights.
- Model picker fed by real `.crfsuite` discovery, with Browse/Rescan.
- Per-candidate tabs (`#1 (score)`) each showing:
  - the bunsetsu split — **lookup bunsetsu bold/blue, passthrough grey**, the same
    reading as the old Pango markup, plus the annotated (`_は_`) form;
  - **Table** view — the dense per-token grid from the GTK panel: predicted label,
    transition scores per inter-token gap, emission scores per CRF label,
    feature values, and (in debug) feature weights;
  - **Canvas** view — the new part:
    - one column per character, one narrow gap column per inter-token boundary;
    - a **boundary confidence bar** per gap, with predicted boundaries drawn as
      full-height red guide lines;
    - an **emission heat strip** per CRF label (blue = negative, red = positive);
    - zoom presets (80 %–150 %), hover highlighting and a readout panel showing
      the hovered token's emissions/features or the hovered gap's transition scores.

### 2. Train

A three-step pipeline mirroring `crf_core.run_training_pipeline`:

1. **Training corpus** — add/remove multiple annotated files, with the
   `きょう _は_ てんき _が_ よい` syntax reminder.
2. **Feature extraction** — toggles for regenerating
   `crf_feature_materials.json` and for appending extended-dictionary entries;
   output TSV path; live progress bar and statistics.
3. **Train CRF model** — algorithm (`lbfgs`, `l2sgd`, `ap`, `pa`, `arow`), `c1`,
   `c2`, `max_iterations`, `feature.possible_transitions`, model output path, and
   a result summary (sentences, tokens, time, iteration, loss, feature count,
   model size). Hyperparameters persist to
   `~/.config/pskk/crf_trainer_params.json`.

A monospace log console at the bottom streams `crf-progress` events from the
backend, colour-coded by level, with an indeterminate/percent progress bar.

Training can run either one-shot from corpus files or from a pre-extracted
features TSV (the two-step workflow from `crf_train_cli.py`).

### 3. Corpus Stats

- Multi-file corpus statistics: lines, sentences, tokens, bunsetsu
  (lookup/passthrough split), characters, and per-sentence averages, with
  per-file rows and a combined summary.
- Sample sentences rendered as bunsetsu splits next to their annotated source.
- **Features TSV inspector**: sentence/token counts, label histogram, the set of
  feature keys found, and a per-sentence table parsed from the intermediate TSV
  written by the extraction step.

---

## Architecture / アーキテクチャ

```
apps/crf-trainer/
├── src-tauri/                  # Rust host
│   └── src/
│       ├── main.rs             # Builder + command registration
│       ├── types.rs            # serde DTOs (mirrored in ui/src/types.ts)
│       └── commands.rs         # Real plumbing + MOCK CRF commands
└── ui/                         # React 18 + TypeScript + Vite
    └── src/
        ├── App.tsx             # Shell, sidebar, environment badges
        ├── api.ts              # Typed invoke/listen wrappers
        ├── types.ts            # TS mirrors of the Rust DTOs
        ├── styles.css
        └── components/
            ├── TestView.tsx        # Test tab
            ├── TrainView.tsx       # 3-step training pipeline
            ├── StatsView.tsx       # Corpus stats + TSV inspector
            ├── NBestPanel.tsx      # Candidate tabs + view switch
            ├── FeatureGrid.tsx     # Numeric per-token grid
            ├── BoundaryCanvas.tsx  # Canvas visualisation
            ├── BunsetsuPreview.tsx
            ├── ModelPicker.tsx
            ├── PipelineStep.tsx
            └── LogConsole.tsx
```

### Why React + TypeScript

The canvas view is driven by layered state (tokens, per-token emissions, the
M×M transition matrix, N-best alternatives, hover/zoom). React keeps that in one
place and redraws the canvas from a `useEffect` on state change, and the TS types
catch payload mismatches that the Python code only guards against at runtime.

---

## Next steps / 次の作業

The seam for the real implementation is deliberately narrow — three functions in
`src-tauri/src/commands.rs`, each marked `MOCK`:

1. **Bridge or port the pipeline.** Either shell out to a Python sidecar
   (`crf_core.run_feature_extraction`, `crf_core.train_model`,
   `crf_core.test_prediction`) or port `crf_core.py` + the CRF maths to Rust.
2. **Stream real progress.** The `crf-progress` event shape already carries
   `stage`/`current`/`total`; forward the Python `progress_callback` into it.
3. **Fill in the prediction payload.** `emission_scores`, `transitions` and
   `boundary_scores` map 1:1 onto `util.crf_compute_emission_scores`,
   `tagger.info().transitions` and the boundary marginals — the canvas needs no
   changes once they are real.

Additional ideas noted while building: cross-validation/accuracy reporting in the
Train tab, a diffs view between two models, and exporting the canvas as PNG.

---

## Reference material / 参考資料

The three Python files that define the model and its original UI live at the
repository root and were copied in for reference:

- `crf_core.py` — parsing, feature extraction, training, prediction (the source of truth)
- `crf_train_cli.py` — the CLI workflow this GUI mirrors
- `conversion_model.py` — the GTK panel this app replaces
