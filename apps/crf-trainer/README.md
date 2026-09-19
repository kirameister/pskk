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

**No Python.** Feature extraction, training and prediction all run in-process in
Rust: the CRF maths come from `crfsuite-compliant-rs` (a Rust port of CRFsuite
0.12) and the features come from `pskk::util` — the same extractor the IME uses
at runtime, so training and inference cannot drift apart.

| Area | Command / feature | Status |
| --- | --- | --- |
| CRF engine | linked in, reported by `get_environment` | **Real (pure Rust)** |
| Model discovery (`.crfsuite` scan) | `list_models` | **Real** |
| Native file dialogs | `pick_corpus_file`, `pick_model_file`, `pick_save_file` | **Real** |
| Corpus parsing + statistics | `load_corpus_report` | **Real** |
| Features-TSV inspector | `inspect_feature_tsv` | **Real** |
| Hyperparameter persistence | `load_training_params`, `save_training_params` | **Real** |
| Feature extraction → TSV | `extract_features` | **Real** |
| CRF training (L-BFGS) | `train_model` | **Real** |
| Prediction (N-best + marginals) | `predict` | **Real** |
| Overwrite guard for the live model | `check_model_target`, `confirm_model_overwrite` | **Real** |

Verified end-to-end on the repository corpus
(`data/crf_training/wagahai_neko_dearu-mecab_processed.txt`, 300 train / 100 test
sentences, real dictionary features): a model trained through this pipeline
reaches **87.0 %** token accuracy on held-out sentences, with 23,171 features and
16 transitions. The test lives in `src/crf.rs` as
`real_corpus_pipeline_round_trip` and skips itself when the corpus is absent.

実コーパスでの統合テスト（300文で訓練・100文で評価、実辞書特徴量）で
トークン精度**87.0%**を確認。テストは`src/crf.rs`の
`real_corpus_pipeline_round_trip`。

### Engine risk / エンジンのリスク

`crfsuite-compliant-rs` is pinned to exactly `=0.4.2` on purpose (5 of its 7
published versions have been yanked, and 0.4.x permits breaking changes). Its
L-BFGS training and Viterbi inference were verified against the reference C
implementation: models trained by both are **byte-identical**, and inference
agrees on **100 %** of labels. Only L-BFGS is wired up for that reason — the
port's online trainers deliberately use a different RNG stream.

`crfsuite-compliant-rs`は`=0.4.2`に固定。L-BFGS訓練とViterbi推論は参照C実装と
突き合わせ済み（訓練モデルはバイト単位で一致、推論ラベルは100%一致）。

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
│       ├── crf.rs              # CRF engine: model I/O, training, inference
│       └── commands.rs         # Tauri commands (host plumbing + pipeline)
└── ui/                         # React 18 + TypeScript + Vite
    └── src/
        ├── App.tsx             # Shell: sidebar, status bar, environment badges
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
catch payload mismatches that would otherwise only surface at runtime.

---

## Next steps / 次の作業

The pipeline is complete and Python-free. What is left is mostly breadth:

1. **More training algorithms.** Only L-BFGS is wired up (it is the parity-verified
   one). The port also ships L2-SGD, averaged perceptron, passive-aggressive and
   AROW; enabling them means accepting that they will not match the C reference,
   because the port uses a different RNG stream for online trainers.
2. **A golden test in CI.** Pin the crate and assert that training on a checked-in
   fixture still reproduces a checked-in golden model byte-for-byte. That turns
   "trust the dependency" into "verify the dependency" on every bump.
3. **Cross-validation / accuracy reporting** in the Train tab, a **model diff**
   view, and exporting the canvas as PNG.
4. **Wiring the IME runtime.** `Convertor::with_crf_model` in `src/henkan.rs` is
   still never called, so the IME currently converts dictionary-only. The model
   reader in `src-tauri/src/crf.rs` shows exactly what that wiring needs; moving it
   into the `pskk` library is the natural next step.

既知の残作業: 他アルゴリズムの配線、CIでのゴールデンテスト、交差検証表示、
そして`with_crf_model`の配線（現状IMEは辞書のみで変換）。

---

## Reference material / 参考資料

The three Python files that define the model and its original UI live at the
repository root and were copied in for reference:

- `crf_core.py` — parsing, feature extraction, training, prediction (the source of truth)
- `crf_train_cli.py` — the CLI workflow this GUI mirrors
- `conversion_model.py` — the GTK panel this app replaces
