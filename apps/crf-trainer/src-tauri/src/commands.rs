//! Tauri command surface for the CRF trainer GUI.
//!
//! CRFトレーナーGUIのTauriコマンド群。
//!
//! # Implementation status / 実装状況
//!
//! This app was scaffolded GUI-first. Commands fall into two groups:
//! このアプリはGUIファーストで雛形を作成した。コマンドは2種類に分かれる:
//!
//! **Real / 実装済み** — pure host/filesystem plumbing, no CRF maths:
//!   `get_environment`, `list_models`, `pick_*`, `load_corpus_report`,
//!   `load_training_params`, `save_training_params`
//!
//! **Mock / モック** — everything that needs `pycrfsuite` or the Python
//! feature-extraction pipeline. These return synthetic but well-formed data so
//! the GUI (including the canvas visualisation) can be designed and reviewed
//! before the bridge exists. Every payload carries `isMock: true`, which the UI
//! renders as a "MOCK" badge.
//!   `extract_features`, `train_model`, `predict`
//!
//! Porting note / 移植メモ: the mock commands are the seam where the Python
//! side (`crf_core.run_feature_extraction`, `crf_core.train_model`,
//! `crf_core.test_prediction`) gets wired in — either by shelling out to a
//! `python3` sidecar or by porting the logic to Rust.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

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

/// Common Japanese particles, used only by the mock predictor.
/// モック予測器でのみ使用する主要な助詞。
const MOCK_PARTICLES: [char; 16] = [
    'は', 'が', 'を', 'に', 'で', 'と', 'へ', 'も', 'の', 'て', 'し', 'ば', 'か', 'ね', 'よ', 'や',
];

// ═══════════════════════════════════════════════════════════════════════
// Environment probing / 環境検出
// ═══════════════════════════════════════════════════════════════════════

/// First `python3`/`python` on PATH, if any.
fn python_binary() -> Option<String> {
    ["python3", "python"].into_iter().find_map(|candidate| {
        let ok = Command::new(candidate)
            .arg("--version")
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false);
        ok.then(|| candidate.to_string())
    })
}

/// `pycrfsuite.__version__`, or `None` when the import fails.
fn pycrfsuite_version(python: &str) -> Option<String> {
    let out = Command::new(python)
        .args([
            "-c",
            "import pycrfsuite, sys; sys.stdout.write(getattr(pycrfsuite, '__version__', 'unknown'))",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!version.is_empty()).then_some(version)
}

fn config_dir() -> PathBuf {
    pskk::util::get_user_config_dir()
}

fn default_model_path() -> PathBuf {
    pskk::util::get_crf_model_path()
}

fn default_features_path() -> PathBuf {
    config_dir().join("crf_model_training_data.tsv")
}

#[tauri::command]
pub fn get_environment() -> EnvironmentInfo {
    let python = python_binary();
    let python_version = python
        .as_deref()
        .and_then(|p| Command::new(p).arg("--version").output().ok())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|v| !v.is_empty());

    let pycrfsuite_available = python
        .as_deref()
        .and_then(pycrfsuite_version)
        .is_some();

    // The `crfsuite` CLI is optional; it is only used for display.
    let crfsuite_version = Command::new("crfsuite")
        .arg("-v")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if text.is_empty() {
                String::from_utf8_lossy(&out.stderr).trim().to_string()
            } else {
                text
            }
        });

    EnvironmentInfo {
        python_available: python.is_some(),
        python_version,
        pycrfsuite_available,
        crfsuite_version,
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
fn parse_annotated_line(line: &str) -> (Vec<String>, Vec<String>, Vec<Bunsetsu>) {
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
// MOCK commands / モックコマンド (TODO: wire to crf_core.py)
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

/// Small deterministic PRNG so mock scores look organic but stay reproducible.
/// モックのスコアを再現可能かつ自然に見せるための簡易PRNG。
struct MockRng(u64);

impl MockRng {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }
    fn next_f64(&mut self) -> f64 {
        // xorshift64*
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        let v = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
        (v >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + self.next_f64() * (hi - lo)
    }
}

fn seed_from(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |acc, b| {
        (acc ^ b as u64).wrapping_mul(0x0000_0100_0000_01B3)
    })
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

/// Mock labelling: content chars form lookup bunsetsu, particles passthrough.
/// モックラベリング: 内容語はルックアップ、助詞はパススルー。
fn mock_labels(tokens: &[String]) -> Vec<String> {
    let mut labels: Vec<String> = Vec::with_capacity(tokens.len());
    let mut prev_kind: Option<char> = None;

    for token in tokens {
        let ch = token.chars().next().unwrap_or(' ');
        let kind = if MOCK_PARTICLES.contains(&ch) { 'P' } else { 'L' };
        if prev_kind == Some(kind) {
            labels.push(format!("I-{kind}"));
        } else {
            labels.push(format!("B-{kind}"));
        }
        prev_kind = Some(kind);
    }
    labels
}

/// Flip one bunsetsu boundary to build a distinct N-best alternative.
/// 文節境界を1つ反転させて別のN-best候補を作る。
fn flip_boundary(labels: &[String], gap: usize) -> Vec<String> {
    let mut out = labels.to_vec();
    if out.len() < 2 {
        return out;
    }
    let idx = (gap % (out.len() - 1)) + 1;
    let kind = out[idx].rsplit('-').next().unwrap_or("L").to_string();
    if out[idx].starts_with('B') {
        out[idx] = format!("I-{kind}");
    } else {
        out[idx] = format!("B-{kind}");
    }
    out
}

fn feature_values(tokens: &[String], index: usize, rng: &mut MockRng, predicted: &str) -> Vec<FeatureValue> {
    let current = tokens.get(index).cloned().unwrap_or_default();
    let left = tokens.get(index.wrapping_sub(1)).cloned();
    let right = tokens.get(index + 1).cloned();

    let raw: [(String, String); 9] = [
        ("char".into(), current.clone()),
        (
            "char_left".into(),
            if index == 0 { "BOS".into() } else { left.clone().unwrap_or_default() },
        ),
        (
            "char_right".into(),
            right.clone().unwrap_or_else(|| "EOS".into()),
        ),
        (
            "bigram_left".into(),
            match &left {
                Some(l) => format!("{l}{current}"),
                None => format!("BOS{current}"),
            },
        ),
        (
            "bigram_right".into(),
            match &right {
                Some(r) => format!("{current}{r}"),
                None => format!("{current}EOS"),
            },
        ),
        ("dict_max_kl_s".into(), format!("{:.2}", rng.range(0.0, 12.0))),
        ("dict_max_kl_e".into(), format!("{:.2}", rng.range(0.0, 12.0))),
        ("dict_entry_ct_s".into(), format!("{}", (rng.range(0.0, 40.0)) as u32)),
        ("dict_entry_ct_e".into(), format!("{}", (rng.range(0.0, 40.0)) as u32)),
    ];

    raw.into_iter()
        .map(|(key, value)| {
            // Bias the weight of the predicted label so the grid looks coherent.
            let bias = if rng.next_f64() > 0.55 { 1.0 } else { -0.4 };
            let _ = predicted;
            FeatureValue {
                weight: Some((rng.range(0.2, 3.4) * bias * 10.0).round() / 10.0),
                key,
                value,
            }
        })
        .collect()
}

/// Mock prediction payload, shaped exactly like the future real one.
/// 将来の実データと同じ形をしたモック予測結果。
#[tauri::command]
pub async fn predict(request: PredictRequest) -> Result<PredictionResult, String> {
    let text = request.input_text.trim().to_string();
    if text.is_empty() {
        return Err("Input text is empty".into());
    }

    let tokens: Vec<String> = text.chars().map(|c| c.to_string()).collect();
    let model_labels = vec![
        "B-L".to_string(),
        "I-L".to_string(),
        "B-P".to_string(),
        "I-P".to_string(),
    ];

    let mut rng = MockRng::new(seed_from(&text));
    let predicted = mock_labels(&tokens);

    // Emissions: highest for the predicted label, plus noise.
    let emission_scores: Vec<Vec<f64>> = predicted
        .iter()
        .map(|label| {
            model_labels
                .iter()
                .map(|candidate| {
                    let base = if candidate == label {
                        rng.range(1.2, 4.5)
                    } else {
                        rng.range(-3.5, 0.6)
                    };
                    (base * 100.0).round() / 100.0
                })
                .collect()
        })
        .collect();

    // Boundary confidence per gap, nudged up where a particle starts.
    let boundary_scores: Vec<f64> = (0..tokens.len().saturating_sub(1))
        .map(|gap| {
            let next_starts = predicted.get(gap + 1).map(|l| l.starts_with('B')).unwrap_or(false);
            let base = if next_starts { rng.range(0.62, 0.98) } else { rng.range(0.02, 0.35) };
            (base * 1000.0).round() / 1000.0
        })
        .collect();

    let transitions: Vec<TransitionScore> = if request.debug {
        model_labels
            .iter()
            .flat_map(|from| {
                model_labels.iter().map(move |to| (from.clone(), to.clone()))
            })
            .map(|(from, to)| {
                let score = if from == to { rng.range(-2.0, 0.0) } else { rng.range(-4.0, 2.0) };
                TransitionScore {
                    from,
                    to,
                    score: (score * 100.0).round() / 100.0,
                }
            })
            .collect()
    } else {
        // Default view: transitions out of each predicted label.
        let mut seen: HashSet<String> = HashSet::new();
        predicted
            .iter()
            .filter(|l| seen.insert((*l).clone()))
            .flat_map(|from| {
                model_labels.iter().map(move |to| (from.clone(), to.clone()))
            })
            .map(|(from, to)| {
                let score = if from == to { rng.range(-2.0, 0.0) } else { rng.range(-4.0, 2.0) };
                TransitionScore {
                    from,
                    to,
                    score: (score * 100.0).round() / 100.0,
                }
            })
            .collect()
    };

    let features: Vec<Vec<FeatureValue>> = (0..tokens.len())
        .map(|i| feature_values(&tokens, i, &mut rng, &predicted[i]))
        .collect();

    // N-best: rank 1 is the mock prediction, later ranks flip one boundary each.
    let n_best = request.n_best.clamp(1, 10) as usize;
    let mut candidates = Vec::with_capacity(n_best);
    for rank in 0..n_best {
        let labels = if rank == 0 {
            predicted.clone()
        } else {
            flip_boundary(&predicted, rank - 1)
        };
        let score = if rank == 0 {
            rng.range(-1.0, 0.0)
        } else {
            -rng.range(0.4, 3.0) * rank as f64
        };
        candidates.push(NBestCandidate {
            rank: rank as u32 + 1,
            score: (score * 1000.0).round() / 1000.0,
            bunsetsu: labels_to_bunsetsu(&tokens, &labels),
            labels,
        });
    }
    candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    for (i, c) in candidates.iter_mut().enumerate() {
        c.rank = i as u32 + 1;
    }

    let model_path = request
        .model_path
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| default_model_path().to_string_lossy().to_string());

    Ok(PredictionResult {
        input_text: text,
        model_path,
        tokens,
        model_labels,
        features,
        emission_scores,
        transitions,
        boundary_scores,
        candidates,
        debug: request.debug,
        is_mock: true,
    })
}

/// Mock feature extraction run, with progress events.
/// モックの特徴量抽出実行（進捗イベント付き）。
#[tauri::command]
pub async fn extract_features(
    app: AppHandle,
    request: ExtractRequest,
) -> Result<FeatureExtractionResult, String> {
    let job = job_id("extract");
    let started = now_millis();

    let stages: [(&str, &str); 6] = [
        ("dictionary", "Regenerating dictionary features (crf_feature_materials.json)..."),
        ("dictionary", "Dictionary features updated (mock)"),
        ("load", "Loading corpus..."),
        ("load", "Loaded corpus (mock)"),
        ("extract", "Extracting features..."),
        ("save", "Features saved (mock)"),
    ];

    let total = stages.len() as u64;
    for (i, (stage, message)) in stages.iter().enumerate() {
        emit_stage(&app, &job, stage, *message, "info", Some(i as u64 + 1), Some(total));
        tokio::time::sleep(std::time::Duration::from_millis(180)).await;
    }

    let mut stats = CorpusStats {
        source: if request.corpus_paths.len() > 1 {
            format!("{} files combined", request.corpus_paths.len())
        } else {
            request.corpus_paths.first().cloned().unwrap_or_default()
        },
        ..Default::default()
    };
    // Reuse the real parser so the numbers on screen are honest.
    for path in &request.corpus_paths {
        let p = Path::new(path);
        if p.is_file() {
            let _ = stats_for_file(p, &mut stats);
        }
    }

    let output_path = request
        .output_path
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| default_features_path().to_string_lossy().to_string());

    let elapsed = (now_millis() - started) as f64 / 1000.0;
    emit_stage(
        &app,
        &job,
        "done",
        format!("Feature extraction finished in {elapsed:.2}s (mock)"),
        "success",
        Some(total),
        Some(total),
    );

    Ok(FeatureExtractionResult {
        success: true,
        output_path: Some(output_path),
        tsv_size_bytes: 0,
        stats,
        dict_entries: if request.include_dictionary { 12_480 } else { 0 },
        dict_tokens: if request.include_dictionary { 41_902 } else { 0 },
        elapsed_secs: elapsed,
        error_message: None,
        is_mock: true,
    })
}

/// Mock training run, with progress events.
/// モックの訓練実行（進捗イベント付き）。
#[tauri::command]
pub async fn train_model(
    app: AppHandle,
    request: TrainRequest,
) -> Result<TrainingResult, String> {
    let job = job_id("train");
    let started = now_millis();

    let has_features = request
        .features_path
        .as_deref()
        .is_some_and(|p| !p.is_empty());
    let source = if has_features {
        "pre-extracted features TSV".to_string()
    } else {
        format!("{} corpus file(s)", request.corpus_paths.len())
    };

    emit_stage(&app, &job, "prepare", format!("Loading data from {source}..."), "info", Some(1), Some(6));

    let iterations = request.params.max_iterations.min(30) as u64;
    let total = 6 + iterations;
    let mut current = 1u64;
    let mut loss = 42.0f64;

    for _ in 0..iterations {
        current += 1;
        loss *= 0.93;
        emit_stage(
            &app,
            &job,
            "train",
            format!("iter {current}  loss={loss:.6}  (mock L-BFGS)"),
            "info",
            Some(current),
            Some(total),
        );
        tokio::time::sleep(std::time::Duration::from_millis(60)).await;
    }

    emit_stage(&app, &job, "save", "Saving model...", "info", Some(total), Some(total));
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let elapsed = (now_millis() - started) as f64 / 1000.0;
    let model_path = request
        .model_path
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| default_model_path().to_string_lossy().to_string());

    emit_stage(
        &app,
        &job,
        "done",
        format!("Training complete in {elapsed:.2}s — model written to {model_path} (mock)"),
        "success",
        Some(total),
        Some(total),
    );

    let mut stats = CorpusStats::default();
    for path in &request.corpus_paths {
        let p = Path::new(path);
        if p.is_file() {
            let _ = stats_for_file(p, &mut stats);
        }
    }

    Ok(TrainingResult {
        success: true,
        model_path: Some(model_path),
        // Nothing is written to disk yet; report an indicative size only.
        model_size_bytes: 1_842_176,
        training_time_secs: elapsed,
        sentence_count: if stats.sentence_count > 0 { stats.sentence_count } else { 18_204 },
        token_count: if stats.total_tokens > 0 { stats.total_tokens } else { 412_887 },
        last_iteration: Some(iterations as u32),
        loss: Some((loss * 1000.0).round() / 1000.0),
        feature_count: Some(1_204_553),
        error_message: None,
        is_mock: true,
    })
}

/// Keys the UI displays, exposed so both sides cannot drift apart.
/// UIが表示する特徴量キー。両側の齟齬を防ぐため公開。
#[tauri::command]
pub fn display_feature_keys() -> Vec<String> {
    DISPLAY_FEATURE_KEYS
        .iter()
        .map(|k| (*k).to_string())
        .collect()
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
    fn mock_prediction_is_well_formed() {
        let tokens: Vec<String> = "きょうは".chars().map(|c| c.to_string()).collect();
        let labels = mock_labels(&tokens);
        assert_eq!(labels.len(), tokens.len());
        assert!(labels[0].starts_with('B'));
        // Particles become the start of a passthrough bunsetsu.
        assert_eq!(labels[3], "B-P");

        let bunsetsu = labels_to_bunsetsu(&tokens, &labels);
        let rebuilt: String = bunsetsu.iter().map(|b| b.text.clone()).collect();
        assert_eq!(rebuilt, "きょうは");
    }

    #[test]
    fn flipping_a_boundary_changes_exactly_one_label() {
        let base = vec![
            "B-L".to_string(),
            "I-L".to_string(),
            "B-P".to_string(),
            "B-L".to_string(),
        ];
        let flipped = flip_boundary(&base, 0);
        assert_eq!(flipped.len(), base.len());
        let differences = base
            .iter()
            .zip(flipped.iter())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(differences, 1);
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

