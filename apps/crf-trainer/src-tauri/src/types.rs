//! Serializable data types shared between the Tauri backend and the React UI.
//!
//! バックエンド（Rust）とフロントエンド（React）の間で共有されるデータ型。
//!
//! These mirror the Python reference implementation:
//!   参考実装（Python）との対応:
//!     - `CorpusStats`      <-> `crf_core.load_corpus()` stats dict
//!     - `TrainingResult`   <-> `crf_core.TrainingResult`
//!     - `PredictionResult` <-> `conversion_model.on_test_prediction()` view model
//!
//! All structs serialize to camelCase so the TypeScript side stays idiomatic.

use serde::{Deserialize, Serialize};

// ─── Environment / 環境情報 ───────────────────────────────────────────

/// Result of probing the host for the CRF toolchain.
/// CRFツールチェーンの検出結果。
///
/// There is no Python probe: learning and testing run on a pure-Rust CRFsuite
/// port inside this app.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentInfo {
    /// Engine identifier shown in the header, e.g.
    /// `crfsuite-compliant-rs 0.4.2 (pure Rust)`.
    pub crf_engine: String,
    pub config_dir: String,
    pub default_model_path: String,
    pub default_features_path: String,
}

// ─── Model registry / モデル一覧 ──────────────────────────────────────

/// A `.crfsuite` model file discoverable on disk.
/// ディスク上で見つかった `.crfsuite` モデルファイル。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub path: String,
    pub file_name: String,
    pub size_bytes: u64,
    /// RFC3339-ish local timestamp, or `None` when unavailable.
    pub modified: Option<String>,
    /// True for the model the IME itself would load (`util::get_crf_model_path`).
    pub is_default: bool,
}

/// What a training run would write to, and whether that clobbers something.
/// 訓練の出力先と、上書きが発生するかどうか。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelTargetInfo {
    /// Resolved absolute path training would write to.
    pub path: String,
    pub exists: bool,
    pub size_bytes: u64,
    pub modified: Option<String>,
    /// True when this is `util::get_crf_model_path()` — the file the IME loads.
    /// The IME works without a CRF model, but replacing this one changes
    /// bunsetsu splitting immediately.
    pub is_live_model_path: bool,
    /// True when the file is one shipped under the install root's data dir.
    pub is_shipped_model: bool,
}

// ─── Corpus / コーパス統計 ────────────────────────────────────────────

/// Statistics for one annotated corpus file.
/// 注釈付きコーパスファイル1つ分の統計。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorpusStats {
    pub source: String,
    pub line_count: u64,
    pub sentence_count: u64,
    pub total_tokens: u64,
    pub total_bunsetsu: u64,
    pub lookup_bunsetsu: u64,
    pub passthrough_bunsetsu: u64,
    pub total_chars: u64,
}

/// One bunsetsu as parsed out of an annotated line.
/// 注釈付き行から解析された1文節。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bunsetsu {
    pub text: String,
    /// `L` (lookup / dictionary conversion) or `P` (passthrough).
    pub kind: String,
    pub is_lookup: bool,
}

/// A parsed sample sentence, for the corpus preview pane.
/// コーパスプレビュー用の解析済みサンプル文。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleSentence {
    pub line_number: u64,
    pub raw: String,
    pub tokens: Vec<String>,
    /// Char-level labels: `B-L`, `I-L`, `B-P`, `I-P`.
    pub labels: Vec<String>,
    pub bunsetsu: Vec<Bunsetsu>,
}

/// Aggregate report for a (possibly multi-file) corpus selection.
/// （複数ファイル可の）コーパス選択に対する集計レポート。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorpusReport {
    pub per_file: Vec<CorpusStats>,
    pub combined: CorpusStats,
    pub samples: Vec<SampleSentence>,
    /// True when more sample sentences exist than were returned.
    pub samples_truncated: bool,
    /// Non-fatal problems (missing files, unreadable lines, ...).
    pub warnings: Vec<String>,
}

// ─── Features TSV inspector / 特徴量TSVインスペクタ ───────────────────

/// One sentence of a `crf_model_training_data.tsv` file.
/// `crf_model_training_data.tsv`の1文。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureTsvSentence {
    pub index: u64,
    pub line_number: u64,
    pub tokens: Vec<String>,
    pub labels: Vec<String>,
    pub bunsetsu: Vec<Bunsetsu>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelCount {
    pub label: String,
    pub count: u64,
}

/// Summary of a pre-extracted features TSV (see `crf_core.save_training_data_tsv`).
/// 事前抽出済み特徴量TSVのサマリ。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureTsvReport {
    pub path: String,
    pub size_bytes: u64,
    pub sentence_count: u64,
    pub token_count: u64,
    /// Distinct feature keys seen across the file, in first-seen order.
    pub feature_keys: Vec<String>,
    pub label_counts: Vec<LabelCount>,
    pub sentences: Vec<FeatureTsvSentence>,
    pub truncated: bool,
    pub warnings: Vec<String>,
}

// ─── Feature extraction / 特徴量抽出 ──────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractRequest {
    pub corpus_paths: Vec<String>,
    pub output_path: Option<String>,
    /// Append extended-dictionary entries as one-bunsetsu training examples.
    pub include_dictionary: bool,
    /// Rebuild `crf_feature_materials.json` before extracting.
    pub regenerate_dictionary_features: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureExtractionResult {
    pub success: bool,
    pub output_path: Option<String>,
    pub tsv_size_bytes: u64,
    pub stats: CorpusStats,
    pub dict_entries: u64,
    pub dict_tokens: u64,
    pub elapsed_secs: f64,
    pub error_message: Option<String>,
    /// True while the backend returns placeholder data (see `commands.rs`).
    pub is_mock: bool,
}

// ─── Training / 訓練 ──────────────────────────────────────────────────

/// CRF hyperparameters, mirroring `crf_core.train_model(params=...)`.
/// CRFハイパーパラメータ（`crf_core.train_model(params=...)`に対応）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainingParams {
    pub algorithm: String,
    /// L1 regularization.
    pub c1: f64,
    /// L2 regularization.
    pub c2: f64,
    pub max_iterations: u32,
    pub feature_possible_transitions: bool,
}

impl Default for TrainingParams {
    fn default() -> Self {
        Self {
            algorithm: "lbfgs".to_string(),
            c1: 1.0,
            c2: 1e-3,
            max_iterations: 100,
            feature_possible_transitions: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainRequest {
    /// One-shot mode: corpus files are loaded and features extracted first.
    pub corpus_paths: Vec<String>,
    /// Two-step mode: train from a pre-extracted features TSV.
    pub features_path: Option<String>,
    pub model_path: Option<String>,
    pub params: TrainingParams,
    pub include_dictionary: bool,
    pub regenerate_dictionary_features: bool,
}

/// Mirrors `crf_core.TrainingResult`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainingResult {
    pub success: bool,
    pub model_path: Option<String>,
    pub model_size_bytes: u64,
    pub training_time_secs: f64,
    pub sentence_count: u64,
    pub token_count: u64,
    pub last_iteration: Option<u32>,
    pub loss: Option<f64>,
    pub feature_count: Option<u64>,
    /// Terminal trainer message, e.g. "L-BFGS terminated with the stopping criteria".
    pub status: Option<String>,
    pub error_message: Option<String>,
    pub is_mock: bool,
}

// ─── Prediction / 予測 ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PredictRequest {
    pub input_text: String,
    pub model_path: Option<String>,
    pub n_best: u32,
    /// Request the heavier payload (all M×M transitions + feature weights).
    pub debug: bool,
}

/// One CRF feature attached to a token position.
/// トークン位置に付与されたCRF特徴量1つ。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureValue {
    pub key: String,
    pub value: String,
    /// `state_features[(key:value, label)]`, `None` when not in the model.
    pub weight: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionScore {
    pub from: String,
    pub to: String,
    pub score: f64,
}

/// One N-best candidate produced by `util.crf_nbest_predict()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NBestCandidate {
    pub rank: u32,
    pub score: f64,
    pub labels: Vec<String>,
    pub bunsetsu: Vec<Bunsetsu>,
}

/// Everything the Test tab needs to render the grid and the canvas.
/// Testタブがグリッドとキャンバスを描画するために必要な全データ。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PredictionResult {
    pub input_text: String,
    pub model_path: String,
    pub tokens: Vec<String>,
    /// Labels the model can emit, e.g. `["B-L", "I-L", "B-P", "I-P"]`.
    pub model_labels: Vec<String>,
    /// `features[t]` = features at token `t`, in display order.
    pub features: Vec<Vec<FeatureValue>>,
    /// `emission_scores[t][label_index]`.
    pub emission_scores: Vec<Vec<f64>>,
    /// All M×M transitions (debug) or only those from the predicted label.
    pub transitions: Vec<TransitionScore>,
    /// Per-gap boundary confidence in `0.0..=1.0`, drawn by the canvas view.
    pub boundary_scores: Vec<f64>,
    pub candidates: Vec<NBestCandidate>,
    pub debug: bool,
    pub is_mock: bool,
}

// ─── Progress events / 進捗イベント ───────────────────────────────────

/// Payload of the `crf-progress` event stream.
/// `crf-progress`イベントストリームのペイロード。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub job_id: String,
    pub stage: String,
    pub message: String,
    pub current: Option<u64>,
    pub total: Option<u64>,
    /// `info` | `warn` | `error` | `success`.
    pub level: String,
}

