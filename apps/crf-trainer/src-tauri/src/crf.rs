//! Pure-Rust CRF engine for the trainer: model I/O, training and prediction.
//!
//! 純RustのCRFエンジン: モデル入出力・訓練・予測。
//!
//! # No Python
//!
//! Learning and testing are handled entirely by `crfsuite-compliant-rs`, a Rust
//! port of CRFsuite 0.12. Its model files are binary-compatible with the C
//! implementation, and for the L-BFGS configuration used here the trained
//! models were verified byte-identical to models trained by `pycrfsuite`, with
//! 100% label agreement on inference (see the app README).
//!
//! # Why feature extraction lives in `pskk::util`
//!
//! Features are produced by the *same* `pskk::util::add_features_per_line` the
//! IME uses at runtime. The classic CRF bug is train/inference skew — features
//! that look equivalent but differ by a separator — so there is deliberately
//! only one implementation of them, shared by both sides.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crfsuite_compliant_rs::crf1d::encode::Crf1dEncoder;
use crfsuite_compliant_rs::crf1d::tag::Crf1dTagger;
use crfsuite_compliant_rs::model::ModelReader;
use crfsuite_compliant_rs::quark::Quark;
use crfsuite_compliant_rs::train;
use crfsuite_compliant_rs::types::{Attribute, Instance, Item};

use pskk::util::{
    self, CrfFeatureMaterials, FeatureRow, StateFeatureWeights, TransitionWeights,
};

/// Version string shown in the UI. Keep in sync with `Cargo.toml`.
/// UIに表示するバージョン文字列。`Cargo.toml`と同期させること。
pub const CRF_ENGINE: &str = "crfsuite-compliant-rs 0.4.2 (pure Rust)";

/// Feature kind tags used by the model's feature tables.
/// モデルの特徴量テーブルが使う種別タグ。
const FEATURE_KIND_STATE: u32 = 0;
const FEATURE_KIND_TRANSITION: u32 = 1;

/// Canonical feature key order, matching the insertion order of
/// `pskk::util::add_features_per_line`.
///
/// `FeatureRow` is a `HashMap`, so without a fixed order the attribute-ID
/// assignment — and therefore the bytes of the trained model — would differ
/// between runs. Sorting also keeps the features TSV stable and diffable.
pub const FEATURE_ORDER: [&str; 12] = [
    "char",
    "char_left",
    "char_right",
    "bigram_left",
    "bigram_right",
    "trigram_left",
    "trigram_right",
    "ctype",
    "dict_max_kl_s",
    "dict_max_kl_e",
    "dict_entry_ct_s",
    "dict_entry_ct_e",
];

/// Deterministically ordered `(key, value)` pairs of one feature row.
/// 特徴量行を決定的な順序の`(key, value)`ペアにする。
pub fn ordered_features(row: &FeatureRow) -> Vec<(String, String)> {
    let mut pairs: Vec<(String, String)> = Vec::with_capacity(row.len());
    for key in FEATURE_ORDER {
        if let Some(value) = row.get(key) {
            pairs.push((key.to_string(), value.clone()));
        }
    }
    // Anything the canonical order does not know about goes last, sorted, so
    // that a future feature cannot silently reorder existing attributes.
    let mut extra: Vec<(&String, &String)> = row
        .iter()
        .filter(|(key, _)| !FEATURE_ORDER.contains(&key.as_str()))
        .collect();
    extra.sort();
    pairs.extend(extra.into_iter().map(|(k, v)| (k.clone(), v.clone())));
    pairs
}

/// The feature string a model attribute is keyed by, e.g. `char:き`.
/// モデル属性のキーになる特徴量文字列（例: `char:き`）。
pub fn feature_key(key: &str, value: &str) -> String {
    format!("{key}:{value}")
}

/// A sentence plus its char-level labels (`B-L`, `I-L`, `B-P`, `I-P`).
/// 文と文字単位ラベル。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabeledSentence {
    pub tokens: Vec<String>,
    pub labels: Vec<String>,
}

/// A sentence read back from a features TSV.
/// 特徴量TSVから読み戻した文。
#[derive(Debug, Clone)]
pub struct FeatureSentence {
    pub tokens: Vec<String>,
    pub labels: Vec<String>,
    pub features: Vec<FeatureRow>,
}

/// Parameters accepted by the trainer.
/// 訓練器が受け取るパラメータ。
#[derive(Debug, Clone)]
pub struct TrainParams {
    pub algorithm: String,
    pub c1: f64,
    pub c2: f64,
    pub max_iterations: i32,
    pub possible_transitions: bool,
}

impl Default for TrainParams {
    fn default() -> Self {
        Self {
            algorithm: "lbfgs".to_string(),
            c1: 1.0,
            c2: 1e-3,
            max_iterations: 100,
            possible_transitions: true,
        }
    }
}

/// A trained model, still in memory.
/// 訓練済みモデル（メモリ上）。
#[derive(Debug, Clone)]
pub struct TrainedModel {
    pub bytes: Vec<u8>,
    pub feature_count: usize,
    pub label_count: usize,
    pub attribute_count: usize,
}

/// Model weights plus the raw bytes needed to build a tagger.
/// モデルの重みと、タガー構築に必要な生バイト列。
pub struct CrfModel {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    pub labels: Vec<String>,
    pub state_features: StateFeatureWeights,
    pub transitions: TransitionWeights,
}

impl CrfModel {
    /// Load a CRFsuite model file into owned weight maps.
    ///
    /// モデルファイルを所有権付きの重みマップとして読み込む。
    /// Enumerates the model the same way the crate's own `dump` does: state
    /// features through the per-attribute refs, transitions through the
    /// per-label refs (the header's feature count is always 0).
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::from_bytes(path.to_path_buf(), bytes)
    }

    pub fn from_bytes(path: PathBuf, bytes: Vec<u8>) -> Result<Self, String> {
        let model = ModelReader::open(&bytes)
            .ok_or_else(|| format!("not a CRFsuite model: {}", path.display()))?;

        let labels: Vec<String> = (0..model.num_labels())
            .map(|id| model.to_label(id as i32).unwrap_or("?").to_string())
            .collect();

        let mut state_features: StateFeatureWeights = HashMap::new();
        for aid in 0..model.num_attrs() as i32 {
            let Some(attribute) = model.to_attr(aid) else {
                continue;
            };
            for fid in model.get_attrref(aid) {
                let Some(feature) = model.get_feature(fid) else {
                    continue;
                };
                if feature.ftype != FEATURE_KIND_STATE {
                    continue;
                }
                if let Some(label) = labels.get(feature.dst as usize) {
                    state_features.insert((attribute.to_string(), label.clone()), feature.weight);
                }
            }
        }

        let mut transitions: TransitionWeights = HashMap::new();
        for lid in 0..model.num_labels() as i32 {
            for fid in model.get_labelref(lid) {
                let Some(feature) = model.get_feature(fid) else {
                    continue;
                };
                if feature.ftype != FEATURE_KIND_TRANSITION {
                    continue;
                }
                if let (Some(from), Some(to)) = (
                    labels.get(feature.src as usize),
                    labels.get(feature.dst as usize),
                ) {
                    transitions.insert((from.clone(), to.clone()), feature.weight);
                }
            }
        }

        Ok(Self {
            path,
            bytes,
            labels,
            state_features,
            transitions,
        })
    }

    /// Boundary marginal P(the token at `position` starts a new bunsetsu | x).
    ///
    /// 境界マージナル（その位置が新しい文節を開始する確率）。
    ///
    /// Uses the crate's forward-backward marginals, which is the number the
    /// canvas view draws. Returned per inter-token gap, i.e. `tokens.len() - 1`
    /// values.
    pub fn boundary_marginals(&self, features: &[FeatureRow]) -> Result<Vec<f64>, String> {
        if features.len() < 2 {
            return Ok(Vec::new());
        }
        let model = ModelReader::open(&self.bytes)
            .ok_or_else(|| format!("not a CRFsuite model: {}", self.path.display()))?;
        let mut tagger = Crf1dTagger::new(&model);

        let mut instance = Instance::new();
        for row in features {
            let mut contents = Vec::new();
            for (key, value) in ordered_features(row) {
                if let Some(aid) = model.to_aid(&feature_key(&key, &value)) {
                    contents.push(Attribute { aid, value: 1.0 });
                }
            }
            instance.items.push(Item { contents });
            instance.labels.push(0);
        }
        tagger.set(&instance);

        let begin_labels: Vec<i32> = self
            .labels
            .iter()
            .enumerate()
            .filter(|(_, label)| label.starts_with('B'))
            .map(|(id, _)| id as i32)
            .collect();

        let mut out = Vec::with_capacity(features.len() - 1);
        for position in 1..features.len() {
            let mut probability = 0.0;
            for label in &begin_labels {
                probability += tagger.marginal_point(*label, position as i32);
            }
            out.push(probability.clamp(0.0, 1.0));
        }
        Ok(out)
    }
}

/// Extract features for one sentence using the IME's own extractor.
/// IME自身の抽出器を使って1文の特徴量を抽出。
pub fn features_for(tokens: &[String], materials: Option<&CrfFeatureMaterials>) -> Vec<FeatureRow> {
    util::add_features_per_line(tokens.to_vec(), materials)
}

/// Write the two-step workflow's intermediate TSV.
///
/// 2ステップワークフローの中間TSVを書き出す。
/// Format matches `crf_core.save_training_data_tsv`:
/// `# Sentence N`, then `token \t label \t key=value ...`, blank line between
/// sentences.
pub fn save_features_tsv(
    sentences: &[LabeledSentence],
    features: &[Vec<FeatureRow>],
    path: &Path,
) -> Result<u64, String> {
    use std::fmt::Write as _;

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
    }

    let mut out = String::new();
    for (index, (sentence, rows)) in sentences.iter().zip(features).enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let _ = writeln!(out, "# Sentence {}", index + 1);
        for ((token, label), row) in sentence.tokens.iter().zip(&sentence.labels).zip(rows) {
            out.push_str(token);
            out.push('\t');
            out.push_str(label);
            for (key, value) in ordered_features(row) {
                out.push('\t');
                let _ = write!(out, "{key}={value}");
            }
            out.push('\n');
        }
    }

    std::fs::write(path, &out).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(out.len() as u64)
}

/// Parse a features TSV back into sentences.
///
/// 特徴量TSVを文に読み戻す。
/// Mirrors `crf_core.load_training_data_tsv`: `#` comments, blank lines between
/// sentences, `token \t label \t key=value ...` rows.
pub fn parse_features_tsv(path: &Path, limit: Option<usize>) -> Result<Vec<FeatureSentence>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;

    let mut sentences: Vec<FeatureSentence> = Vec::new();
    let mut tokens: Vec<String> = Vec::new();
    let mut labels: Vec<String> = Vec::new();
    let mut features: Vec<FeatureRow> = Vec::new();
    fn flush(
        sentences: &mut Vec<FeatureSentence>,
        tokens: &mut Vec<String>,
        labels: &mut Vec<String>,
        features: &mut Vec<FeatureRow>,
    ) {
        if tokens.is_empty() {
            return;
        }
        sentences.push(FeatureSentence {
            tokens: std::mem::take(tokens),
            labels: std::mem::take(labels),
            features: std::mem::take(features),
        });
    }

    for (index, raw) in text.lines().enumerate() {
        let line_number = index as u64 + 1;
        let line = raw.trim_end_matches('\r');
        if line.trim().is_empty() {
            flush(&mut sentences, &mut tokens, &mut labels, &mut features);
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 2 {
            return Err(format!(
                "{}:{}: malformed row (expected token<TAB>label)",
                path.display(),
                line_number
            ));
        }
        let mut row = FeatureRow::new();
        for field in &parts[2..] {
            if let Some((key, value)) = field.split_once('=') {
                row.insert(key.to_string(), value.to_string());
            }
        }
        tokens.push(parts[0].to_string());
        labels.push(parts[1].to_string());
        features.push(row);
    }
    flush(&mut sentences, &mut tokens, &mut labels, &mut features);

    if let Some(limit) = limit {
        sentences.truncate(limit);
    }
    Ok(sentences)
}

/// Train a CRF model from pre-extracted features.
///
/// 抽出済み特徴量からCRFモデルを訓練。
///
/// Only `lbfgs` is wired up: it is the configuration whose output was verified
/// byte-identical to the C implementation. The online trainers in the port use a
/// different RNG stream by design, so they would not match the reference.
pub fn train(
    sentences: &[LabeledSentence],
    features: &[Vec<FeatureRow>],
    params: &TrainParams,
    log: &mut train::LogFn,
) -> Result<TrainedModel, String> {
    if sentences.is_empty() || features.is_empty() {
        return Err("No training data".to_string());
    }
    if sentences.len() != features.len() {
        return Err(format!(
            "Sentence/feature count mismatch: {} vs {}",
            sentences.len(),
            features.len()
        ));
    }
    if !params.algorithm.eq_ignore_ascii_case("lbfgs") {
        return Err(format!(
            "Algorithm '{}' is not wired up yet; only 'lbfgs' is available \
             (it is the only algorithm verified against the reference implementation).",
            params.algorithm
        ));
    }

    let mut label_quark = Quark::new();
    let mut attribute_quark = Quark::new();
    let mut instances: Vec<Instance> = Vec::with_capacity(sentences.len());

    for (sentence, rows) in sentences.iter().zip(features) {
        if sentence.tokens.len() != rows.len() || rows.len() != sentence.labels.len() {
            return Err(format!(
                "Token/feature/label length mismatch in sentence with {} tokens",
                sentence.tokens.len()
            ));
        }
        let mut instance = Instance::new();
        for (label, row) in sentence.labels.iter().zip(rows) {
            let contents = ordered_features(row)
                .into_iter()
                .map(|(key, value)| Attribute {
                    aid: attribute_quark.get(&feature_key(&key, &value)),
                    value: 1.0,
                })
                .collect();
            instance.items.push(Item { contents });
            instance.labels.push(label_quark.get(label));
        }
        instances.push(instance);
    }

    let mut encoder = Crf1dEncoder::new(
        &instances,
        label_quark.num(),
        attribute_quark.num(),
        0.0,
        false,
        params.possible_transitions,
    );

    // CRFsuite's L-BFGS defaults, so the numbers stay comparable with the
    // reference implementation.
    let weights = train::lbfgs::train_lbfgs(
        &mut encoder,
        &instances,
        params.c1,
        params.c2,
        params.max_iterations,
        6,
        1e-5,
        10,
        1e-5,
        "MoreThuente",
        20,
        log,
        None,
    );

    let label_strings: Vec<String> = (0..label_quark.num())
        .map(|id| label_quark.to_string(id as i32).unwrap_or("?").to_string())
        .collect();
    let attribute_strings: Vec<String> = (0..attribute_quark.num())
        .map(|id| attribute_quark.to_string(id as i32).unwrap_or("?").to_string())
        .collect();

    let bytes = encoder.save_model(&weights, &label_strings, &attribute_strings);
    if bytes.is_empty() {
        return Err("The trainer returned an empty model".to_string());
    }

    Ok(TrainedModel {
        feature_count: weights.len(),
        label_count: label_strings.len(),
        attribute_count: attribute_strings.len(),
        bytes,
    })
}

/// One parsed line of trainer output.
/// 訓練器出力の1行を解析したもの。
#[derive(Debug, Clone, PartialEq)]
pub enum TrainingLogEvent {
    Iteration {
        index: u32,
        loss: Option<f64>,
        active_features: Option<u64>,
    },
    /// Terminal status line, e.g. "L-BFGS terminated with the stopping criteria".
    Status(String),
}

/// Interpret a `crfsuite-compliant-rs` L-BFGS log callback.
///
/// `crfsuite-compliant-rs`のL-BFGSログコールバックを解釈する。
/// The trainer hands over one multi-line block per iteration, so a single call
/// can contain the iteration number, loss and feature counts.
pub fn parse_training_log(message: &str) -> Option<TrainingLogEvent> {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(rest) = trimmed.split("Iteration #").nth(1) {
        let index = rest
            .split(|c: char| !c.is_ascii_digit())
            .next()
            .and_then(|n| n.parse::<u32>().ok())?;
        let number = |prefix: &str| -> Option<f64> {
            trimmed
                .lines()
                .find_map(|line| line.strip_prefix(prefix))
                .and_then(|v| v.trim().parse::<f64>().ok())
        };
        return Some(TrainingLogEvent::Iteration {
            index,
            loss: number("Loss:"),
            active_features: number("Active features:").map(|v| v as u64),
        });
    }

    if trimmed.starts_with("L-BFGS") {
        return Some(TrainingLogEvent::Status(
            trimmed.lines().next().unwrap_or(trimmed).to_string(),
        ));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sentence(text: &str) -> LabeledSentence {
        LabeledSentence {
            tokens: text.chars().map(|c| c.to_string()).collect(),
            labels: Vec::new(),
        }
    }

    #[test]
    fn ordered_features_follow_the_canonical_order() {
        let mut row = FeatureRow::new();
        row.insert("dict_max_kl_s".into(), "3.5".into());
        row.insert("char".into(), "き".into());
        row.insert("char_left".into(), "BOS".into());
        row.insert("zz_custom".into(), "1".into());
        row.insert("bigram_left".into(), "BOS き".into());

        let pairs = ordered_features(&row);
        let keys: Vec<&str> = pairs.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            keys,
            vec!["char", "char_left", "bigram_left", "dict_max_kl_s", "zz_custom"],
            "unknown keys must come last, sorted"
        );
        assert!(keys.iter().position(|k| *k == "zz_custom") == Some(keys.len() - 1));
    }

    #[test]
    fn feature_key_uses_a_colon_separator() {
        assert_eq!(feature_key("char", "き"), "char:き");
        assert_eq!(feature_key("bigram_left", "BOS き"), "bigram_left:BOS き");
    }

    #[test]
    fn tsv_round_trips_sentences_and_features() {
        let dir = std::env::temp_dir().join("pskk-crf-tsv-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("features.tsv");

        let a = LabeledSentence {
            tokens: vec!["き".into(), "ょ".into()],
            labels: vec!["B-L".into(), "I-L".into()],
        };
        let b = LabeledSentence {
            tokens: vec!["は".into()],
            labels: vec!["B-P".into()],
        };
        let mut row_a0 = FeatureRow::new();
        row_a0.insert("char".into(), "き".into());
        row_a0.insert("char_left".into(), "BOS".into());
        let mut row_a1 = FeatureRow::new();
        row_a1.insert("char".into(), "ょ".into());
        let mut row_b0 = FeatureRow::new();
        row_b0.insert("char".into(), "は".into());

        let written = save_features_tsv(
            &[a.clone(), b.clone()],
            &[vec![row_a0, row_a1], vec![row_b0]],
            &path,
        )
        .expect("write tsv");
        assert!(written > 0);

        let parsed = parse_features_tsv(&path, None).expect("parse tsv");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].tokens, a.tokens);
        assert_eq!(parsed[0].labels, a.labels);
        assert_eq!(parsed[0].features.len(), 2);
        assert_eq!(parsed[0].features[0].get("char").map(String::as_str), Some("き"));
        assert_eq!(parsed[0].features[0].get("char_left").map(String::as_str), Some("BOS"));
        assert_eq!(parsed[1].labels, vec!["B-P".to_string()]);
    }

    #[test]
    fn tsv_parse_reports_malformed_rows() {
        let dir = std::env::temp_dir().join("pskk-crf-tsv-bad");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.tsv");
        std::fs::write(&path, "token_without_label\n").unwrap();
        let err = parse_features_tsv(&path, None).unwrap_err();
        assert!(err.contains("malformed"), "unexpected error: {err}");
    }

    #[test]
    fn training_log_blocks_are_parsed() {
        let block = "***** Iteration #7 *****\nLoss: 42.123456\nFeature norm: 1.5\nError norm: 0.25\nActive features: 1234\nLine search trials: 1\nLine search step: 0.5\nSeconds required for this iteration: 0.000\n";
        assert_eq!(
            parse_training_log(block),
            Some(TrainingLogEvent::Iteration {
                index: 7,
                loss: Some(42.123456),
                active_features: Some(1234),
            })
        );
        assert_eq!(
            parse_training_log("L-BFGS terminated with the stopping criteria\n"),
            Some(TrainingLogEvent::Status(
                "L-BFGS terminated with the stopping criteria".to_string()
            ))
        );
        assert_eq!(parse_training_log("\n"), None);
        assert_eq!(parse_training_log("Total seconds required for training: 0.000"), None);
    }

    #[test]
    fn training_produces_a_loadable_model() {
        // A tiny, perfectly separable task: a boundary always follows "は".
        let sentences = vec![
            LabeledSentence {
                tokens: "きょうはよい".chars().map(|c| c.to_string()).collect(),
                labels: vec!["B-L", "I-L", "I-L", "B-P", "B-L", "I-L"]
                    .into_iter()
                    .map(str::to_string)
                    .collect(),
            },
            LabeledSentence {
                tokens: "あすはあめ".chars().map(|c| c.to_string()).collect(),
                labels: vec!["B-L", "I-L", "B-P", "B-L", "I-L"]
                    .into_iter()
                    .map(str::to_string)
                    .collect(),
            },
        ];
        let materials = CrfFeatureMaterials::default();
        let features: Vec<Vec<FeatureRow>> = sentences
            .iter()
            .map(|s| features_for(&s.tokens, Some(&materials)))
            .collect();

        // c1 = 0 on purpose: L1 regularisation prunes hard on a fixture this
        // small (with c1 = 1.0 only a couple of features survive), and this test
        // is about the feature tables round-tripping, not about sparsity.
        let params = TrainParams {
            c1: 0.0,
            max_iterations: 10,
            ..TrainParams::default()
        };
        let mut log: train::LogFn = Box::new(|_| {});
        let trained = train(&sentences, &features, &params, &mut log).expect("train");

        assert!(trained.feature_count > 0);
        // The fixture only uses B-L, I-L and B-P, so exactly those get IDs.
        assert_eq!(trained.label_count, 3);
        assert!(!trained.bytes.is_empty());

        let model = CrfModel::from_bytes(PathBuf::from("memory"), trained.bytes).expect("load");
        assert_eq!(model.labels.len(), 3);
        for label in ["B-L", "I-L", "B-P"] {
            assert!(model.labels.iter().any(|l| l == label), "missing label {label}");
        }
        assert!(!model.state_features.is_empty(), "state features must be readable");
        // Transitions are stored per-label; a 3-label model has up to 9.
        assert!(!model.transitions.is_empty(), "transitions must be readable");

        // Feature weights must be addressable exactly as the UI looks them up.
        let (key, value) = ordered_features(&features[0][0])
            .into_iter()
            .next()
            .expect("a feature");
        let wanted = feature_key(&key, &value);
        let hit = model
            .state_features
            .get(&(wanted.clone(), "B-L".to_string()));
        assert!(
            hit.is_some(),
            "expected a stored weight for {wanted} / B-L in a model with {} state features",
            model.state_features.len()
        );
    }

    #[test]
    fn boundary_marginals_are_probabilities_per_gap() {
        let sentences = vec![LabeledSentence {
            tokens: "きょうは".chars().map(|c| c.to_string()).collect(),
            labels: vec!["B-L", "I-L", "I-L", "B-P"]
                .into_iter()
                .map(str::to_string)
                .collect(),
        }];
        let materials = CrfFeatureMaterials::default();
        let features: Vec<Vec<FeatureRow>> = sentences
            .iter()
            .map(|s| features_for(&s.tokens, Some(&materials)))
            .collect();
        let params = TrainParams {
            max_iterations: 5,
            ..TrainParams::default()
        };
        let mut log: train::LogFn = Box::new(|_| {});
        let trained = train(&sentences, &features, &params, &mut log).expect("train");
        let model = CrfModel::from_bytes(PathBuf::from("memory"), trained.bytes).expect("load");

        let marginals = model.boundary_marginals(&features[0]).expect("marginals");
        assert_eq!(marginals.len(), features[0].len() - 1);
        for value in marginals {
            assert!((0.0..=1.0).contains(&value), "marginal out of range: {value}");
        }
    }

    #[test]
    fn unsupported_algorithms_are_rejected_clearly() {
        let s = LabeledSentence {
            tokens: vec!["き".into()],
            labels: vec!["B-L".into()],
        };
        let features = vec![vec![FeatureRow::new()]];
        let params = TrainParams {
            algorithm: "ap".into(),
            ..TrainParams::default()
        };
        let mut log: train::LogFn = Box::new(|_| {});
        let err = train(&[s], &features, &params, &mut log).unwrap_err();
        assert!(err.contains("lbfgs"), "unexpected error: {err}");
    }

    #[test]
    fn engine_label_is_reported() {
        assert!(CRF_ENGINE.contains("pure Rust"));
        assert!(!CRF_ENGINE.to_lowercase().contains("python"));
    }

    #[test]
    fn chars_helper_is_used() {
        assert_eq!(sentence("あい").tokens, vec!["あ", "い"]);
    }

    /// End-to-end run on the real corpus: parse → extract → train → predict.
    ///
    /// 実コーパスでの統合テスト: 解析 → 抽出 → 訓練 → 予測。
    /// This is the check that proves the Python-free path actually learns
    /// something: it uses the real annotated corpus, the real dictionary-derived
    /// feature materials and the real model file format. It skips itself when
    /// the corpus is absent (e.g. in a packaged build).
    #[test]
    fn real_corpus_pipeline_round_trip() {
        let corpus = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../data/crf_training/wagahai_neko_dearu-mecab_processed.txt");
        if !corpus.is_file() {
            eprintln!("skipping: {} is not present", corpus.display());
            return;
        }
        let text = std::fs::read_to_string(&corpus).expect("read corpus");

        let mut parsed: Vec<LabeledSentence> = Vec::new();
        for line in text.lines() {
            let (tokens, labels, _) = crate::commands::parse_annotated_line(line);
            if !tokens.is_empty() {
                parsed.push(LabeledSentence { tokens, labels });
            }
        }
        assert!(parsed.len() > 400, "corpus looks too small: {}", parsed.len());

        // Real dictionary-derived materials, exactly as the app loads them.
        let materials = util::load_crf_feature_materials(None).unwrap_or_default();

        let train_set = &parsed[..300];
        let test_set = &parsed[300..400];
        let train_features: Vec<Vec<FeatureRow>> = train_set
            .iter()
            .map(|s| features_for(&s.tokens, Some(&materials)))
            .collect();

        let params = TrainParams {
            max_iterations: 20,
            ..TrainParams::default()
        };
        let mut log: train::LogFn = Box::new(|_| {});
        let trained = train(train_set, &train_features, &params, &mut log)
            .expect("train on real corpus");
        let model =
            CrfModel::from_bytes(PathBuf::from("memory"), trained.bytes.clone()).expect("load");

        // Predict every held-out sentence the way the `predict` command does.
        let mut correct = 0usize;
        let mut total = 0usize;
        let mut marginal_checks = 0usize;
        for gold in test_set {
            let rows = features_for(&gold.tokens, Some(&materials));
            let emissions =
                util::crf_compute_emission_scores(&rows, &model.state_features, &model.labels);
            let paths = util::crf_nbest_viterbi(&emissions, &model.transitions, &model.labels, 3);
            assert!(!paths.is_empty(), "no candidates for {:?}", gold.tokens);
            let best = &paths[0].labels;
            assert_eq!(best.len(), gold.tokens.len());
            for (predicted, expected) in best.iter().zip(&gold.labels) {
                total += 1;
                if predicted == expected {
                    correct += 1;
                }
            }
            if gold.tokens.len() > 2 && marginal_checks < 5 {
                let marginals = model.boundary_marginals(&rows).expect("marginals");
                assert_eq!(marginals.len(), gold.tokens.len() - 1);
                assert!(marginals.iter().all(|p| (0.0..=1.0).contains(p)));
                marginal_checks += 1;
            }
        }

        let accuracy = correct as f64 / total as f64;
        println!(
            "real corpus: {} train / {} test sentences, {} features, {} labels, {} transitions, token accuracy {:.1}%",
            train_set.len(),
            test_set.len(),
            trained.feature_count,
            model.labels.len(),
            model.transitions.len(),
            accuracy * 100.0
        );
        // Well above the ~25% you would get from picking one of four labels.
        assert!(accuracy > 0.5, "token accuracy too low: {accuracy:.3}");
    }
}
