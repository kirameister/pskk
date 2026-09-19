//! Tauri command surface for the CRF trainer GUI.
//!
//! CRFトレーナーGUIのTauriコマンド群。
//!
//! # No Python / Python非依存
//!
//! Feature extraction, training and prediction are all implemented in Rust:
//! the CRF maths come from `crfsuite-compliant-rs` (see `crate::crf`) and the
//! features come from `pskk::util`, the same code the IME uses at runtime.
//! Nothing here spawns an interpreter.
//!
//! **Commands / コマンド**
//!   host plumbing : `get_environment`, `list_models`, `pick_*`,
//!                   `load_corpus_report`, `inspect_feature_tsv`,
//!                   `check_model_target`, `confirm_model_overwrite`,
//!                   `load_training_params`, `save_training_params`
//!   CRF pipeline  : `extract_features`, `train_model`, `predict`

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crfsuite_compliant_rs::train;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use pskk::util::{CrfFeatureMaterials, FeatureRow};

use crate::crf::{self, LabeledSentence};
use crate::types::*;

/// Event name used for all progress/log traffic.
/// 全ての進捗・ログ通信用のイベント名。
pub const PROGRESS_EVENT: &str = "crf-progress";

/// Feature keys displayed in the Test tab grid, in display order.
/// Testタブのグリッドに表示する特徴量キー（表示順）。
const DISPLAY_FEATURE_KEYS: [&str; 9] = [
    "char",
    "char_left",
    "char_right",
    "bigram_left",
    "bigram_right",
    "dict_max_kl_s",
    "dict_max_kl_e",
    "dict_entry_ct_s",
    "dict_entry_ct_e",
];

// ═══════════════════════════════════════════════════════════════════════
// Environment probing / 環境検出
// ═══════════════════════════════════════════════════════════════════════

fn config_dir() -> PathBuf {
    pskk::util::get_user_config_dir()
}

fn default_model_path() -> PathBuf {
    pskk::util::get_crf_model_path()
}

fn default_features_path() -> PathBuf {
    config_dir().join("crf_model_training_data.tsv")
}

/// Report the host paths and the CRF engine backing this app.
///
/// ホストのパスと、このアプリが使うCRFエンジンを報告する。
/// There is no interpreter probe: the engine is linked in, so the only thing
/// that can really be "missing" is a trained model.
#[tauri::command]
pub fn get_environment() -> EnvironmentInfo {
    EnvironmentInfo {
        crf_engine: crf::CRF_ENGINE.to_string(),
        config_dir: config_dir().to_string_lossy().to_string(),
        default_model_path: default_model_path().to_string_lossy().to_string(),
        default_features_path: default_features_path().to_string_lossy().to_string(),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Model discovery / モデル探索
// ═══════════════════════════════════════════════════════════════════════

/// Directory holding the models that ship with the install.
/// インストールに同梱されるモデルのディレクトリ。
fn shipped_models_dir() -> PathBuf {
    pskk::util::get_datadir().join("data").join("crf_training")
}

/// Resolve a requested model path, falling back to the IME's model path.
/// 要求されたモデルパスを解決し、未指定ならIMEのモデルパスにフォールバック。
fn resolve_model_target(path: Option<&str>) -> PathBuf {
    match path.map(str::trim) {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => default_model_path(),
    }
}

/// Authoritative facts about what a training run would write to.
/// 訓練が書き込む先についての確実な情報。
fn model_target_info(target: &Path) -> ModelTargetInfo {
    let meta = std::fs::metadata(target).ok().filter(|m| m.is_file());
    ModelTargetInfo {
        path: target.to_string_lossy().to_string(),
        exists: meta.is_some(),
        size_bytes: meta.as_ref().map(|m| m.len()).unwrap_or(0),
        modified: meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .map(format_system_time),
        is_live_model_path: target == default_model_path(),
        is_shipped_model: target.starts_with(shipped_models_dir()),
    }
}

/// Report whether the training output path already holds a model file.
///
/// 訓練の出力先に既存のモデルファイルがあるかを報告する。
/// The UI uses this to warn *before* the user presses Train.
#[tauri::command]
pub fn check_model_target(path: Option<String>) -> ModelTargetInfo {
    model_target_info(&resolve_model_target(path.as_deref()))
}

/// Ask the user to confirm overwriting an existing model file.
///
/// 既存のモデルファイルを上書きしてよいか確認する。
///
/// Returns `true` when it is safe to proceed: either nothing is there to
/// overwrite, or the user accepted the warning dialog. The CRF model is an
/// optional aid — the IME falls back to dictionary-only conversion without it —
/// so the wording says so rather than treating the loss as fatal.
#[tauri::command]
pub async fn confirm_model_overwrite(
    app: AppHandle,
    path: Option<String>,
) -> Result<bool, String> {
    let target = resolve_model_target(path.as_deref());
    let info = model_target_info(&target);
    if !info.exists {
        // Nothing to lose: skip the prompt entirely.
        return Ok(true);
    }

    let mut detail = format!(
        "{}\n\nExisting file: {} (last modified {})\n",
        info.path,
        human_bytes(info.size_bytes),
        info.modified
            .as_deref()
            .map(format_epoch_seconds)
            .unwrap_or_else(|| "unknown".to_string()),
    );

    if info.is_live_model_path {
        detail.push_str(
            "\nThis is the model the IME loads. The IME does not depend on it — \
             without a CRF model it falls back to dictionary-only conversion — but \
             bunsetsu splitting changes the moment this file is replaced.\n",
        );
    }
    if info.is_shipped_model {
        detail.push_str(
            "\nThis file ships with PSKK: writing here may need root, and the next \
             install will restore the original.\n",
        );
    }
    detail.push_str("\nOverwrite it?");

    let mut builder = app
        .dialog()
        .message(detail)
        .title("Overwrite existing CRF model?")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Overwrite".to_string(),
            "Cancel".to_string(),
        ));
    // Attach to the main window so the dialog is modal to it.
    if let Some(window) = app.get_webview_window("main") {
        builder = builder.parent(&window);
    }

    Ok(builder.blocking_show())
}

/// Format a byte count for dialog text.
/// ダイアログ用にバイト数を整形。
fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Render an epoch-seconds string (see `format_system_time`) as a UTC date.
/// `format_system_time` の出力（エポック秒）をUTC日付に変換。
///
/// Hand-rolled to avoid pulling in a date crate for one dialog line.
fn format_epoch_seconds(raw: &str) -> String {
    let Ok(secs) = raw.parse::<i64>() else {
        return raw.to_string();
    };
    let days = secs.div_euclid(86_400);
    let time_of_day = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        time_of_day / 3600,
        (time_of_day % 3600) / 60
    )
}

/// Howard Hinnant's `civil_from_days`: days since 1970-01-01 → (y, m, d).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[tauri::command]
pub fn list_models() -> Vec<ModelInfo> {
    let default = default_model_path();
    let mut dirs: Vec<PathBuf> = vec![config_dir()];
    let models_dir = config_dir().join("models");
    if models_dir.is_dir() {
        dirs.push(models_dir);
    }
    // Models shipped with the install (`/opt/pskk/data/crf_training`), so a fresh
    // machine can test predictions before training anything itself.
    let shipped_models = shipped_models_dir();
    if shipped_models.is_dir() {
        dirs.push(shipped_models);
    }

    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut models: Vec<ModelInfo> = Vec::new();

    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("crfsuite") {
                continue;
            }
            let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
            if !seen.insert(canonical) {
                continue;
            }
            let meta = entry.metadata().ok();
            models.push(ModelInfo {
                file_name: path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
                path: path.to_string_lossy().to_string(),
                size_bytes: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                modified: meta
                    .as_ref()
                    .and_then(|m| m.modified().ok())
                    .map(format_system_time),
                is_default: path == default,
            });
        }
    }

    // Default model first, then most recently modified.
    models.sort_by(|a, b| {
        b.is_default
            .cmp(&a.is_default)
            .then_with(|| b.modified.cmp(&a.modified))
    });
    models
}

fn format_system_time(time: SystemTime) -> String {
    let secs = time
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Keep it dependency-free: the UI only needs a stable, sortable string.
    format!("{secs}")
}

// ═══════════════════════════════════════════════════════════════════════
// File dialogs / ファイルダイアログ
// ═══════════════════════════════════════════════════════════════════════

#[tauri::command]
pub async fn pick_corpus_file(app: AppHandle) -> Result<Option<String>, String> {
    let picked = app
        .dialog()
        .file()
        .set_title("Select training corpus or features TSV")
        .add_filter("Corpus / features", &["txt", "tsv"])
        .add_filter("All files", &["*"])
        .blocking_pick_file();
    file_path_to_string(picked)
}

#[tauri::command]
pub async fn pick_model_file(app: AppHandle) -> Result<Option<String>, String> {
    let picked = app
        .dialog()
        .file()
        .set_title("Select CRF model")
        .add_filter("CRF model", &["crfsuite"])
        .add_filter("All files", &["*"])
        .blocking_pick_file();
    file_path_to_string(picked)
}

#[tauri::command]
pub async fn pick_save_file(
    app: AppHandle,
    title: String,
    default_path: Option<String>,
    extension: Option<String>,
) -> Result<Option<String>, String> {
    let mut builder = app.dialog().file().set_title(title);
    if let Some(name) = extension.as_deref() {
        builder = builder.add_filter("Output", &[name]);
    }
    if let Some(path) = default_path.as_deref() {
        let path = Path::new(path);
        if let Some(dir) = path.parent() {
            builder = builder.set_directory(dir);
        }
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            builder = builder.set_file_name(name);
        }
    }
    file_path_to_string(builder.blocking_save_file())
}

fn file_path_to_string(picked: Option<tauri_plugin_dialog::FilePath>) -> Result<Option<String>, String> {
    match picked {
        None => Ok(None),
        Some(path) => path
            .into_path()
            .map(|p| Some(p.to_string_lossy().to_string()))
            .map_err(|e| format!("Unsupported file path: {e}")),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Training parameters / 訓練パラメータの永続化
// ═══════════════════════════════════════════════════════════════════════

fn params_path() -> PathBuf {
    config_dir().join("crf_trainer_params.json")
}

#[tauri::command]
pub fn load_training_params() -> TrainingParams {
    std::fs::read_to_string(params_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

#[tauri::command]
pub fn save_training_params(params: TrainingParams) -> Result<(), String> {
    let path = params_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(&params).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

// ═══════════════════════════════════════════════════════════════════════
// Corpus parsing / コーパス解析 (real)
// ═══════════════════════════════════════════════════════════════════════

/// Parse one annotated line into char tokens + 4-class labels.
/// 注釈付き1行を文字トークンと4クラスラベルに解析。
///
/// Mirrors `crf_core.parse_annotated_line`:
///   `きょう _は_ てんき` → tokens `['き','ょ','う','は',...]`
///                          labels `['B-L','I-L','I-L','B-P',...]`
pub(crate) fn parse_annotated_line(line: &str) -> (Vec<String>, Vec<String>, Vec<Bunsetsu>) {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return (Vec::new(), Vec::new(), Vec::new());
    }

    let mut tokens = Vec::new();
    let mut labels = Vec::new();
    let mut bunsetsu = Vec::new();

    for part in trimmed.split_whitespace() {
        let is_passthrough = part.starts_with('_') || part.ends_with('_');
        let text = part.trim_matches('_');
        if text.is_empty() {
            continue;
        }
        let kind = if is_passthrough { "P" } else { "L" };

        for (i, ch) in text.chars().enumerate() {
            tokens.push(ch.to_string());
            labels.push(if i == 0 {
                format!("B-{kind}")
            } else {
                format!("I-{kind}")
            });
        }
        bunsetsu.push(Bunsetsu {
            text: text.to_string(),
            kind: kind.to_string(),
            is_lookup: !is_passthrough,
        });
    }

    (tokens, labels, bunsetsu)
}

fn stats_for_file(path: &Path, stats: &mut CorpusStats) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        stats.line_count += 1;

        let (tokens, labels, bunsetsu) = parse_annotated_line(trimmed);
        if tokens.is_empty() {
            continue;
        }
        stats.sentence_count += 1;
        stats.total_tokens += tokens.len() as u64;
        stats.total_chars += tokens.iter().map(|t| t.chars().count() as u64).sum::<u64>();
        stats.total_bunsetsu += bunsetsu.len() as u64;
        stats.lookup_bunsetsu += labels.iter().filter(|l| l.starts_with("B-L")).count() as u64;
        stats.passthrough_bunsetsu += labels.iter().filter(|l| l.starts_with("B-P")).count() as u64;
    }
    Ok(())
}

/// Load and summarise one or more corpus files, with sample sentences.
/// 1つ以上のコーパスファイルを読み込み、サンプル文付きで集計。
#[tauri::command]
pub fn load_corpus_report(paths: Vec<String>, sample_limit: usize) -> CorpusReport {
    let mut per_file = Vec::new();
    let mut combined = CorpusStats {
        source: if paths.len() > 1 {
            format!("{} files combined", paths.len())
        } else {
            paths.first().cloned().unwrap_or_default()
        },
        ..Default::default()
    };
    let mut samples = Vec::new();
    let mut warnings = Vec::new();
    let mut total_samples = 0usize;

    for path in &paths {
        let p = Path::new(path);
        if !p.is_file() {
            warnings.push(format!("File not found: {path}"));
            continue;
        }
        let mut stats = CorpusStats {
            source: path.clone(),
            ..Default::default()
        };
        match stats_for_file(p, &mut stats) {
            Ok(()) => {}
            Err(e) => {
                warnings.push(e);
                continue;
            }
        }

        // Sample sentences are only collected until the UI budget is filled.
        if samples.len() < sample_limit {
            if let Ok(text) = std::fs::read_to_string(p) {
                for (idx, line) in text.lines().enumerate() {
                    if samples.len() >= sample_limit {
                        break;
                    }
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.starts_with('#') {
                        continue;
                    }
                    let (tokens, labels, bunsetsu) = parse_annotated_line(trimmed);
                    if tokens.is_empty() {
                        continue;
                    }
                    samples.push(SampleSentence {
                        line_number: idx as u64 + 1,
                        raw: trimmed.to_string(),
                        tokens,
                        labels,
                        bunsetsu,
                    });
                    total_samples += 1;
                }
            }
        }

        combined.line_count += stats.line_count;
        combined.sentence_count += stats.sentence_count;
        combined.total_tokens += stats.total_tokens;
        combined.total_bunsetsu += stats.total_bunsetsu;
        combined.lookup_bunsetsu += stats.lookup_bunsetsu;
        combined.passthrough_bunsetsu += stats.passthrough_bunsetsu;
        combined.total_chars += stats.total_chars;
        per_file.push(stats);
    }

    // Detect truncation by re-counting only when a file had more sentences.
    let samples_truncated = per_file
        .iter()
        .map(|s| s.sentence_count)
        .sum::<u64>()
        > total_samples as u64;

    CorpusReport {
        per_file,
        combined,
        samples,
        samples_truncated,
        warnings,
    }
}

/// Parse and summarise a pre-extracted features TSV.
/// 事前抽出済み特徴量TSVを解析して要約する。
///
/// Mirrors `crf_core.load_training_data_tsv`: `token \t label \t key=value ...`,
/// `#` comment lines, blank lines separating sentences.
#[tauri::command]
pub fn inspect_feature_tsv(path: String, limit: usize) -> Result<FeatureTsvReport, String> {
    let p = Path::new(&path);
    if !p.is_file() {
        return Err(format!("Features TSV not found: {path}"));
    }
    let size_bytes = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    let text = std::fs::read_to_string(p).map_err(|e| format!("{path}: {e}"))?;

    let mut sentences: Vec<FeatureTsvSentence> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut feature_keys: Vec<String> = Vec::new();
    let mut label_counts: Vec<LabelCount> = Vec::new();
    let mut token_count: u64 = 0;

    let mut tokens: Vec<String> = Vec::new();
    let mut labels: Vec<String> = Vec::new();
    let mut start_line: u64 = 0;

    let flush = |tokens: &mut Vec<String>,
                     labels: &mut Vec<String>,
                     start_line: u64,
                     sentences: &mut Vec<FeatureTsvSentence>| {
        if tokens.is_empty() {
            return;
        }
        let bunsetsu = labels_to_bunsetsu(tokens, labels);
        sentences.push(FeatureTsvSentence {
            index: sentences.len() as u64 + 1,
            line_number: start_line,
            tokens: std::mem::take(tokens),
            labels: std::mem::take(labels),
            bunsetsu,
        });
    };

    for (idx, raw_line) in text.lines().enumerate() {
        let line_number = idx as u64 + 1;
        let line = raw_line.trim_end_matches('\r');
        if line.trim().is_empty() {
            flush(&mut tokens, &mut labels, start_line, &mut sentences);
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 2 {
            if warnings.len() < 20 {
                warnings.push(format!("Line {line_number}: malformed (expected token<TAB>label)"));
            }
            continue;
        }
        if tokens.is_empty() {
            start_line = line_number;
        }
        let label = parts[1].to_string();
        match label_counts.iter_mut().find(|c| c.label == label) {
            Some(entry) => entry.count += 1,
            None => label_counts.push(LabelCount { label, count: 1 }),
        }
        for feature in &parts[2..] {
            if let Some((key, _)) = feature.split_once('=') {
                if !feature_keys.iter().any(|k| k == key) {
                    feature_keys.push(key.to_string());
                }
            }
        }
        tokens.push(parts[0].to_string());
        labels.push(parts[1].to_string());
        token_count += 1;
    }
    flush(&mut tokens, &mut labels, start_line, &mut sentences);

    let sentence_count = sentences.len() as u64;
    let truncated = sentences.len() > limit;
    sentences.truncate(limit);

    Ok(FeatureTsvReport {
        path,
        size_bytes,
        sentence_count,
        token_count,
        feature_keys,
        label_counts,
        sentences,
        truncated,
        warnings,
    })
}

// ═══════════════════════════════════════════════════════════════════════
// CRF pipeline / CRFパイプライン
//
// Pure Rust: `crfsuite-compliant-rs` for the model maths and `pskk::util` for
// the features. No interpreter, no subprocess.
// 純Rust実装。モデル演算は`crfsuite-compliant-rs`、特徴量は`pskk::util`。
// ═══════════════════════════════════════════════════════════════════════

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn job_id(prefix: &str) -> String {
    format!("{prefix}-{}", now_millis())
}

fn emit(app: &AppHandle, event: ProgressEvent) {
    // A failed emit (no window listening) must not abort the job.
    let _ = app.emit(PROGRESS_EVENT, event);
}

#[allow(clippy::too_many_arguments)]
fn emit_stage(
    app: &AppHandle,
    job: &str,
    stage: &str,
    message: impl Into<String>,
    level: &str,
    current: Option<u64>,
    total: Option<u64>,
) {
    emit(
        app,
        ProgressEvent {
            job_id: job.to_string(),
            stage: stage.to_string(),
            message: message.into(),
            current,
            total,
            level: level.to_string(),
        },
    );
}

/// Derive bunsetsu spans from a char-level label sequence.
/// 文字レベルのラベル列から文節スパンを再構成。
fn labels_to_bunsetsu(tokens: &[String], labels: &[String]) -> Vec<Bunsetsu> {
    let mut out: Vec<Bunsetsu> = Vec::new();
    for (token, label) in tokens.iter().zip(labels.iter()) {
        let kind = label.rsplit('-').next().unwrap_or("L");
        let starts_new = label.starts_with('B') || out.is_empty();
        if starts_new {
            out.push(Bunsetsu {
                text: token.clone(),
                kind: kind.to_string(),
                is_lookup: kind == "L",
            });
        } else if let Some(last) = out.last_mut() {
            last.text.push_str(token);
        }
    }
    out
}

/// Resolve the dictionary-derived feature materials used by extraction.
///
/// 抽出に使う辞書由来の特徴量マテリアルを解決する。
/// Regeneration is best-effort: if it fails we fall back to the existing file
/// rather than refusing to extract.
fn feature_materials(
    regenerate: bool,
    log: &mut Vec<String>,
) -> Result<CrfFeatureMaterials, String> {
    if regenerate {
        match pskk::util::generate_crf_feature_materials(None) {
            Ok((path, materials)) => {
                log.push(format!(
                    "Dictionary features regenerated: {}",
                    path.display()
                ));
                return Ok(materials);
            }
            Err(error) => log.push(format!(
                "Warning: could not regenerate dictionary features ({error}); using the existing file"
            )),
        }
    }
    pskk::util::load_crf_feature_materials(None).map_err(|e| e.to_string())
}

/// Read annotated corpus files into labeled sentences.
/// 注釈付きコーパスを文単位で読み込む。
fn load_corpus_sentences(paths: &[String], log: &mut Vec<String>) -> Vec<LabeledSentence> {
    let mut sentences = Vec::new();
    for path in paths {
        let file_path = Path::new(path);
        if !file_path.is_file() {
            log.push(format!("Warning: file not found: {path}"));
            continue;
        }
        let text = match std::fs::read_to_string(file_path) {
            Ok(text) => text,
            Err(error) => {
                log.push(format!("Warning: could not read {path}: {error}"));
                continue;
            }
        };
        let before = sentences.len();
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let (tokens, labels, _) = parse_annotated_line(trimmed);
            if tokens.is_empty() {
                continue;
            }
            sentences.push(LabeledSentence { tokens, labels });
        }
        log.push(format!(
            "Loaded {} sentences from {}",
            sentences.len() - before,
            path
        ));
    }
    sentences
}

/// Extended-dictionary readings as single-bunsetsu lookup examples.
///
/// 拡張辞書の読みを「1文節のルックアップ例」として訓練データに加える。
/// Mirrors `crf_core.load_extended_dictionary_as_training_data`: every reading
/// becomes one bunsetsu labelled `B-L` / `I-L`.
fn dictionary_sentences() -> (Vec<LabeledSentence>, u64) {
    let path = config_dir().join("extended_dictionary.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return (Vec::new(), 0);
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return (Vec::new(), 0);
    };
    let Some(entries) = value.as_object() else {
        return (Vec::new(), 0);
    };

    let mut sentences = Vec::with_capacity(entries.len());
    let mut tokens_total = 0u64;
    for reading in entries.keys() {
        let tokens = pskk::util::tokenize_line(reading);
        if tokens.is_empty() {
            continue;
        }
        let labels = tokens
            .iter()
            .enumerate()
            .map(|(index, _)| {
                if index == 0 {
                    "B-L".to_string()
                } else {
                    "I-L".to_string()
                }
            })
            .collect();
        tokens_total += tokens.len() as u64;
        sentences.push(LabeledSentence { tokens, labels });
    }
    (sentences, tokens_total)
}

/// Emit coarse progress for a long loop.
/// 長いループの粗い進捗を発行する。
fn emit_progress(app: &AppHandle, job: &str, stage: &str, done: u64, total: u64, unit: &str) {
    emit_stage(
        app,
        job,
        stage,
        format!("{done}/{total} {unit}"),
        "info",
        Some(done),
        Some(total),
    );
}

/// Extract features from the selected corpora and write the intermediate TSV.
///
/// 選択したコーパスから特徴量を抽出し、中間TSVを書き出す。
#[tauri::command]
pub async fn extract_features(
    app: AppHandle,
    request: ExtractRequest,
) -> Result<FeatureExtractionResult, String> {
    let job = job_id("extract");
    let started = Instant::now();
    let mut log: Vec<String> = Vec::new();

    emit_stage(
        &app,
        &job,
        "dictionary",
        "Preparing dictionary features...",
        "info",
        Some(0),
        None,
    );
    let materials = feature_materials(request.regenerate_dictionary_features, &mut log)?;
    for line in &log {
        let level = if line.starts_with("Warning") { "warn" } else { "info" };
        emit_stage(&app, &job, "dictionary", line.clone(), level, None, None);
    }

    // ── Corpus ──
    if request.corpus_paths.is_empty() {
        return Err("No corpus files selected".to_string());
    }
    let mut log_lines = Vec::new();
    let mut sentences = load_corpus_sentences(&request.corpus_paths, &mut log_lines);
    for line in &log_lines {
        let level = if line.starts_with("Warning") { "warn" } else { "info" };
        emit_stage(&app, &job, "load", line.clone(), level, None, None);
    }
    if sentences.is_empty() {
        return Err("No sentences found in the selected corpus files".to_string());
    }

    // Statistics come from the same parser the Corpus Stats tab uses.
    let report = load_corpus_report(request.corpus_paths.clone(), 0);
    let mut stats = report.combined.clone();
    for warning in &report.warnings {
        emit_stage(&app, &job, "load", warning.clone(), "warn", None, None);
    }

    // ── Optional dictionary entries ──
    let (dict_sentences, dict_tokens) = if request.include_dictionary {
        dictionary_sentences()
    } else {
        (Vec::new(), 0)
    };
    let dict_entries = dict_sentences.len() as u64;
    if dict_entries > 0 {
        emit_stage(
            &app,
            &job,
            "load",
            format!("Added {dict_entries} dictionary entries ({dict_tokens} tokens)"),
            "info",
            None,
            None,
        );
        stats.sentence_count += dict_entries;
        stats.total_tokens += dict_tokens;
        stats.total_chars += dict_tokens;
        stats.total_bunsetsu += dict_entries;
        stats.lookup_bunsetsu += dict_entries;
        sentences.extend(dict_sentences);
    }

    // ── Feature extraction ──
    let total = sentences.len() as u64;
    emit_stage(
        &app,
        &job,
        "extract",
        format!("Extracting features for {total} sentences..."),
        "info",
        Some(0),
        Some(total),
    );
    let mut features: Vec<Vec<FeatureRow>> = Vec::with_capacity(sentences.len());
    for (index, sentence) in sentences.iter().enumerate() {
        features.push(crf::features_for(&sentence.tokens, Some(&materials)));
        if index % 200 == 0 {
            emit_progress(&app, &job, "extract", index as u64, total, "sentences");
        }
    }
    emit_progress(&app, &job, "extract", total, total, "sentences");

    // ── Intermediate TSV ──
    let output_path = request
        .output_path
        .filter(|p| !p.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_features_path);
    let tsv_size_bytes = crf::save_features_tsv(&sentences, &features, &output_path)?;

    let elapsed_secs = started.elapsed().as_secs_f64();
    emit_stage(
        &app,
        &job,
        "done",
        format!(
            "Wrote {} sentences to {} ({elapsed_secs:.2}s)",
            sentences.len(),
            output_path.display()
        ),
        "success",
        Some(total),
        Some(total),
    );

    Ok(FeatureExtractionResult {
        success: true,
        output_path: Some(output_path.to_string_lossy().to_string()),
        tsv_size_bytes,
        stats,
        dict_entries,
        dict_tokens,
        elapsed_secs,
        error_message: None,
        is_mock: false,
    })
}

/// Mutable state shared with the trainer's log callback.
/// 訓練器のログコールバックと共有する可変状態。
#[derive(Default)]
struct TrainLogState {
    last_iteration: Option<u32>,
    last_loss: Option<f64>,
    status: String,
}

/// Train a CRF model, either from corpus files (one-shot) or a features TSV.
///
/// コーパスから（ワンショット）または特徴量TSVからCRFモデルを訓練。
#[tauri::command]
pub async fn train_model(
    app: AppHandle,
    request: TrainRequest,
) -> Result<TrainingResult, String> {
    let job = job_id("train");
    let started = Instant::now();

    let params = crf::TrainParams {
        algorithm: request.params.algorithm.clone(),
        c1: request.params.c1,
        c2: request.params.c2,
        max_iterations: request.params.max_iterations.max(1) as i32,
        possible_transitions: request.params.feature_possible_transitions,
    };

    // ── Data source ──
    let features_path = request
        .features_path
        .clone()
        .filter(|p| !p.trim().is_empty());

    let (sentences, features) = match &features_path {
        Some(path) => {
            emit_stage(
                &app,
                &job,
                "prepare",
                format!("Loading pre-extracted features from {path}..."),
                "info",
                Some(0),
                None,
            );
            let parsed = crf::parse_features_tsv(Path::new(path), None)?;
            if parsed.is_empty() {
                return Err(format!("No training sentences found in {path}"));
            }
            let sentences: Vec<LabeledSentence> = parsed
                .iter()
                .map(|s| LabeledSentence {
                    tokens: s.tokens.clone(),
                    labels: s.labels.clone(),
                })
                .collect();
            let features: Vec<Vec<FeatureRow>> =
                parsed.into_iter().map(|s| s.features).collect();
            (sentences, features)
        }
        None => {
            if request.corpus_paths.is_empty() {
                return Err("Select corpus files or a features TSV first".to_string());
            }
            // One-shot: corpus → features → train, writing the intermediate TSV
            // exactly like the two-step workflow would.
            let mut log = Vec::new();
            let materials = feature_materials(request.regenerate_dictionary_features, &mut log)?;
            for line in &log {
                let level = if line.starts_with("Warning") { "warn" } else { "info" };
                emit_stage(&app, &job, "prepare", line.clone(), level, None, None);
            }
            let mut log = Vec::new();
            let mut sentences = load_corpus_sentences(&request.corpus_paths, &mut log);
            for line in &log {
                let level = if line.starts_with("Warning") { "warn" } else { "info" };
                emit_stage(&app, &job, "prepare", line.clone(), level, None, None);
            }
            if request.include_dictionary {
                let (extra, tokens) = dictionary_sentences();
                if !extra.is_empty() {
                    emit_stage(
                        &app,
                        &job,
                        "prepare",
                        format!(
                            "Added {} dictionary entries ({tokens} tokens)",
                            extra.len()
                        ),
                        "info",
                        None,
                        None,
                    );
                    sentences.extend(extra);
                }
            }
            if sentences.is_empty() {
                return Err("No sentences found in the selected corpus files".to_string());
            }
            let features: Vec<Vec<FeatureRow>> = sentences
                .iter()
                .map(|s| crf::features_for(&s.tokens, Some(&materials)))
                .collect();
            let tsv_path = default_features_path();
            match crf::save_features_tsv(&sentences, &features, &tsv_path) {
                Ok(_) => emit_stage(
                    &app,
                    &job,
                    "prepare",
                    format!("Training data saved to {}", tsv_path.display()),
                    "info",
                    None,
                    None,
                ),
                Err(error) => emit_stage(
                    &app,
                    &job,
                    "prepare",
                    format!("Warning: could not save the intermediate TSV: {error}"),
                    "warn",
                    None,
                    None,
                ),
            }
            (sentences, features)
        }
    };

    let token_count: u64 = sentences.iter().map(|s| s.tokens.len() as u64).sum();
    let sentence_count = sentences.len() as u64;
    let max_iterations = params.max_iterations.max(1) as u64;

    emit_stage(
        &app,
        &job,
        "train",
        format!(
            "Training CRF model ({}, c1={}, c2={}) on {sentence_count} sentences, {token_count} tokens...",
            params.algorithm, params.c1, params.c2
        ),
        "info",
        Some(0),
        Some(max_iterations),
    );

    // ── Train, streaming the trainer's own log through the progress channel ──
    let state = Rc::new(RefCell::new(TrainLogState::default()));
    let state_for_log = Rc::clone(&state);
    let app_for_log = app.clone();
    let job_for_log = job.clone();
    let mut log: train::LogFn = Box::new(move |message: &str| {
        let Some(event) = crf::parse_training_log(message) else {
            return;
        };
        let mut state = state_for_log.borrow_mut();
        match event {
            crf::TrainingLogEvent::Iteration {
                index,
                loss,
                active_features,
            } => {
                state.last_iteration = Some(index);
                if loss.is_some() {
                    state.last_loss = loss;
                }
                let mut text = format!("iteration {index}");
                if let Some(loss) = loss {
                    text.push_str(&format!("  loss={loss:.6}"));
                }
                if let Some(active) = active_features {
                    text.push_str(&format!("  active features={active}"));
                }
                emit_stage(
                    &app_for_log,
                    &job_for_log,
                    "train",
                    text,
                    "info",
                    Some(index as u64),
                    Some(max_iterations),
                );
            }
            crf::TrainingLogEvent::Status(status) => {
                state.status = status.clone();
                let level = if status.contains("error") { "error" } else { "info" };
                emit_stage(&app_for_log, &job_for_log, "train", status, level, None, None);
            }
        }
    });

    let trained = match crf::train(&sentences, &features, &params, &mut log) {
        Ok(trained) => trained,
        Err(error) => {
            emit_stage(&app, &job, "done", error.clone(), "error", None, None);
            return Err(error);
        }
    };

    // ── Write the model ──
    let model_path = request
        .model_path
        .filter(|p| !p.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_model_path);
    if let Some(parent) = model_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("{}: {e}", parent.display()))?;
        }
    }
    std::fs::write(&model_path, &trained.bytes)
        .map_err(|e| format!("{}: {e}", model_path.display()))?;

    let (last_iteration, last_loss, status) = {
        let state = state.borrow();
        (state.last_iteration, state.last_loss, state.status.clone())
    };
    let training_time_secs = started.elapsed().as_secs_f64();
    emit_stage(
        &app,
        &job,
        "done",
        format!(
            "Model written to {} ({} bytes, {} features, {} labels, {} attributes)",
            model_path.display(),
            trained.bytes.len(),
            trained.feature_count,
            trained.label_count,
            trained.attribute_count
        ),
        "success",
        Some(max_iterations),
        Some(max_iterations),
    );

    Ok(TrainingResult {
        success: true,
        model_path: Some(model_path.to_string_lossy().to_string()),
        model_size_bytes: trained.bytes.len() as u64,
        training_time_secs,
        sentence_count,
        token_count,
        last_iteration,
        loss: last_loss,
        feature_count: Some(trained.feature_count as u64),
        status: (!status.is_empty()).then_some(status),
        error_message: None,
        is_mock: false,
    })
}

/// Predict bunsetsu splits for the input text.
///
/// 入力テキストの文節分割を予測する。
/// Everything returned is computed from the model: emissions and transitions
/// come from its weights, the N-best list from the repo's Viterbi, and the
/// boundary bars from forward-backward marginals.
#[tauri::command]
pub async fn predict(request: PredictRequest) -> Result<PredictionResult, String> {
    let text = request.input_text.trim().to_string();
    if text.is_empty() {
        return Err("Input text is empty".to_string());
    }

    let model_path = request
        .model_path
        .filter(|p| !p.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_model_path);
    if !model_path.is_file() {
        return Err(format!(
            "No model at {}. Train one in the Train tab first.",
            model_path.display()
        ));
    }
    let model = crf::CrfModel::load(&model_path)?;

    let tokens = pskk::util::tokenize_line(&text);
    if tokens.is_empty() {
        return Err("Input text produced no tokens".to_string());
    }

    let materials = pskk::util::load_crf_feature_materials(None).unwrap_or_default();
    let features = crf::features_for(&tokens, Some(&materials));

    // Emission scores and the N-best list are computed by the repo's own CRF
    // helpers, driven by the weights read out of the model file.
    let emission_scores =
        pskk::util::crf_compute_emission_scores(&features, &model.state_features, &model.labels);

    let n_best = request.n_best.clamp(1, 10) as usize;
    let best_paths =
        pskk::util::crf_nbest_viterbi(&emission_scores, &model.transitions, &model.labels, n_best);

    let best_labels = best_paths
        .first()
        .map(|path| path.labels.clone())
        .unwrap_or_default();

    // Boundary confidence: P(the next token starts a new bunsetsu).
    let boundary_scores = model.boundary_marginals(&features).unwrap_or_default();

    // Feature values with the weight the *best* path assigns to them.
    let feature_rows: Vec<Vec<FeatureValue>> = features
        .iter()
        .enumerate()
        .map(|(index, row)| {
            DISPLAY_FEATURE_KEYS
                .iter()
                .filter_map(|key| {
                    let value = row.get(*key)?;
                    let weight = best_labels.get(index).and_then(|label| {
                        model
                            .state_features
                            .get(&(crf::feature_key(key, value), label.clone()))
                            .copied()
                    });
                    Some(FeatureValue {
                        key: (*key).to_string(),
                        value: value.clone(),
                        weight,
                    })
                })
                .collect()
        })
        .collect();

    // Transitions: every pair in debug mode, otherwise only those leaving a
    // label the best path actually uses.
    let used: HashSet<&String> = best_labels.iter().collect();
    let mut transitions: Vec<TransitionScore> = model
        .transitions
        .iter()
        .filter(|((from, _), _)| request.debug || used.contains(from))
        .map(|((from, to), score)| TransitionScore {
            from: from.clone(),
            to: to.clone(),
            score: *score,
        })
        .collect();
    transitions.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| a.to.cmp(&b.to)));

    let candidates: Vec<NBestCandidate> = best_paths
        .iter()
        .enumerate()
        .map(|(index, path)| NBestCandidate {
            rank: index as u32 + 1,
            score: path.score,
            bunsetsu: labels_to_bunsetsu(&tokens, &path.labels),
            labels: path.labels.clone(),
        })
        .collect();

    Ok(PredictionResult {
        input_text: text,
        model_path: model_path.to_string_lossy().to_string(),
        tokens,
        model_labels: model.labels,
        features: feature_rows,
        emission_scores,
        transitions,
        boundary_scores,
        candidates,
        debug: request.debug,
        is_mock: false,
    })
}

/// Keys the UI displays, exposed so both sides cannot drift apart.
/// UIが表示する特徴量キー。両側の齟齬を防ぐため公開。
#[tauri::command]
pub fn display_feature_keys() -> Vec<String> {
    DISPLAY_FEATURE_KEYS.iter().map(|k| (*k).to_string()).collect()
}

// ═══════════════════════════════════════════════════════════════════════
// Tests / テスト
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn write_temp(name: &str, contents: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("pskk-crf-trainer-{name}"));
        std::fs::write(&path, contents).expect("write temp fixture");
        path
    }

    #[test]
    fn parses_lookup_and_passthrough_labels() {
        let (tokens, labels, bunsetsu) = parse_annotated_line("きょう _は_ てんき _が_ よい");

        assert_eq!(tokens.len(), labels.len());
        assert_eq!(tokens.first().map(String::as_str), Some("き"));
        assert_eq!(
            labels,
            vec!["B-L", "I-L", "I-L", "B-P", "B-L", "I-L", "I-L", "B-P", "B-L", "I-L"]
        );
        assert_eq!(bunsetsu.len(), 5);
        assert_eq!(bunsetsu[0].text, "きょう");
        assert!(bunsetsu[0].is_lookup);
        assert_eq!(bunsetsu[1].kind, "P");
        assert!(!bunsetsu[1].is_lookup);
    }

    #[test]
    fn skips_comments_and_blank_lines() {
        let (tokens, _, _) = parse_annotated_line("# a comment");
        assert!(tokens.is_empty());
        let (tokens, _, _) = parse_annotated_line("   ");
        assert!(tokens.is_empty());
    }

    #[test]
    fn corpus_report_accumulates_statistics() {
        let corpus = write_temp(
            "corpus.txt",
            "# header\nきょう _は_ てんき _が_ よい\n\nわたし _は_ がくせい _です_\n",
        );
        let report = load_corpus_report(vec![corpus.to_string_lossy().to_string()], 5);

        assert!(report.warnings.is_empty(), "unexpected warnings: {:?}", report.warnings);
        assert_eq!(report.per_file.len(), 1);
        assert_eq!(report.combined.sentence_count, 2);
        // 5 bunsetsu in sentence 1 (3 lookup + 2 passthrough) and 4 in sentence 2
        // (2 lookup + 2 passthrough).
        assert_eq!(report.combined.total_bunsetsu, 9);
        assert_eq!(report.combined.lookup_bunsetsu, 5);
        assert_eq!(report.combined.passthrough_bunsetsu, 4);
        assert_eq!(report.samples.len(), 2);
        assert!(!report.samples_truncated);
        assert_eq!(report.samples[0].bunsetsu[1].text, "は");
    }

    #[test]
    fn corpus_report_reports_missing_files() {
        let report = load_corpus_report(vec!["/nonexistent/corpus.txt".into()], 5);
        assert_eq!(report.per_file.len(), 0);
        assert_eq!(report.warnings.len(), 1);
    }

    #[test]
    fn feature_tsv_inspector_reads_the_saved_format() {
        let tsv = write_temp(
            "features.tsv",
            "# Sentence 1\nき\tB-L\tchar=き\tchar_left=BOS\nょ\tI-L\tchar=ょ\tchar_left=き\n\n\
             # Sentence 2\nは\tB-P\tchar=は\tchar_left=BOS\n",
        );
        let report = inspect_feature_tsv(tsv.to_string_lossy().to_string(), 10)
            .expect("inspect features tsv");

        assert_eq!(report.sentence_count, 2);
        assert_eq!(report.token_count, 3);
        assert_eq!(report.sentences[0].tokens, vec!["き", "ょ"]);
        assert_eq!(report.sentences[0].bunsetsu.len(), 1);
        assert_eq!(report.sentences[0].bunsetsu[0].text, "きょ");
        assert!(report.feature_keys.contains(&"char_left".to_string()));
        // B-L, I-L and B-P all occur in the fixture.
        assert_eq!(report.label_counts.len(), 3);
        assert!(report.warnings.is_empty(), "unexpected warnings: {:?}", report.warnings);
    }

    #[test]
    fn feature_tsv_inspector_truncates_sample_sentences() {
        let tsv = write_temp(
            "features-truncated.tsv",
            "# Sentence 1\nき\tB-L\tchar=き\n\n# Sentence 2\nは\tB-P\tchar=は\n",
        );
        let report = inspect_feature_tsv(tsv.to_string_lossy().to_string(), 1)
            .expect("inspect features tsv");

        assert_eq!(report.sentence_count, 2);
        assert_eq!(report.sentences.len(), 1);
        assert!(report.truncated);
    }



    #[test]
    fn display_feature_keys_match_the_grid_order() {
        let keys = display_feature_keys();
        assert_eq!(keys.first().map(String::as_str), Some("char"));
        assert_eq!(keys.last().map(String::as_str), Some("dict_entry_ct_e"));
        assert_eq!(keys.len(), 9);
    }

    #[test]
    fn model_target_resolution_falls_back_to_the_live_model_path() {
        let expected = default_model_path().to_string_lossy().to_string();

        assert_eq!(resolve_model_target(None), default_model_path());
        assert_eq!(resolve_model_target(Some("")), default_model_path());
        assert_eq!(resolve_model_target(Some("   ")), default_model_path());
        assert_eq!(resolve_model_target(Some("/tmp/custom.crfsuite")), PathBuf::from("/tmp/custom.crfsuite"));

        // Whichever way it is requested, the live model path is flagged as such.
        let info = check_model_target(None);
        assert_eq!(info.path, expected);
        assert!(info.is_live_model_path);
    }

    #[test]
    fn model_target_reports_an_existing_file() {
        let existing = write_temp("model-target.crfsuite", "not really a model");
        let info = check_model_target(Some(existing.to_string_lossy().to_string()));

        assert!(info.exists);
        assert_eq!(info.size_bytes, 18);
        assert!(info.modified.is_some());
        // A temp path is neither the live model nor a shipped one.
        assert!(!info.is_live_model_path);
        assert!(!info.is_shipped_model);
    }

    #[test]
    fn model_target_reports_a_missing_file() {
        let missing = std::env::temp_dir().join("pskk-crf-trainer-does-not-exist.crfsuite");
        let _ = std::fs::remove_file(&missing);
        let info = check_model_target(Some(missing.to_string_lossy().to_string()));

        assert!(!info.exists);
        assert_eq!(info.size_bytes, 0);
        assert!(info.modified.is_none());
    }

    #[test]
    fn human_bytes_formats_readably() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(999), "999 B");
        assert_eq!(human_bytes(1024), "1.0 KB");
        assert_eq!(human_bytes(1_275_404), "1.2 MB");
    }

    #[test]
    fn epoch_seconds_render_as_a_utc_date() {
        // 2024-05-17T12:34:56Z
        assert_eq!(format_epoch_seconds("1715949296"), "2024-05-17 12:34 UTC");
        // Leap day, and a non-numeric value passes through untouched.
        assert_eq!(format_epoch_seconds("1709205896"), "2024-02-29 11:24 UTC");
        assert_eq!(format_epoch_seconds("unparseable"), "unparseable");
    }
}

