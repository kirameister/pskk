use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub const PACKAGE_NAME: &str = "pskk";
pub const VERSION: &str = "0.0.1";
pub const DEFAULT_INSTALL_ROOT: &str = "/opt/pskk";

pub type CandidateCounts = HashMap<String, i32>;
pub type Dictionary = HashMap<String, CandidateCounts>;
pub type FeatureRow = HashMap<String, String>;
pub type StateFeatureWeights = HashMap<(String, String), f64>;
pub type TransitionWeights = HashMap<(String, String), f64>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharType {
    Hiragana,
    Katakana,
    Kanji,
    Ascii,
    Other,
}

impl CharType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hiragana => "hiragana",
            Self::Katakana => "katakana",
            Self::Kanji => "kanji",
            Self::Ascii => "ascii",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrfFeatureMaterials {
    pub max_key_len_starting_with: HashMap<String, usize>,
    pub max_key_len_ending_with: HashMap<String, usize>,
    pub dict_entry_count_starting_with: HashMap<String, usize>,
    pub dict_entry_count_ending_with: HashMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NBestPath {
    pub labels: Vec<String>,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BunsetsuSegment {
    pub text: String,
    pub label: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DictionaryBuildStats {
    pub files_processed: usize,
    pub total_readings: usize,
    pub total_candidates: usize,
    pub okurigana_entries_expanded: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtendedDictionaryStats {
    pub files_processed: usize,
    pub yomi_kanji_mappings: usize,
    pub kanchoku_kanji_count: usize,
    pub source_entries_scanned: usize,
    pub total_readings: usize,
    pub total_candidates: usize,
}

#[derive(Debug)]
pub enum UtilError {
    Io(io::Error),
    Json(serde_json::Error),
    InvalidConfig(&'static str),
    MissingField(&'static str),
}

impl fmt::Display for UtilError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Json(err) => write!(f, "{err}"),
            Self::InvalidConfig(message) => write!(f, "{message}"),
            Self::MissingField(field) => write!(f, "missing required field: {field}"),
        }
    }
}

impl std::error::Error for UtilError {}

impl From<io::Error> for UtilError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for UtilError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

pub fn char_type(c: char) -> CharType {
    let cp = c as u32;
    if (0x3040..=0x309F).contains(&cp) {
        CharType::Hiragana
    } else if (0x30A0..=0x30FF).contains(&cp) {
        CharType::Katakana
    } else if (0x4E00..=0x9FFF).contains(&cp) || (0x3400..=0x4DBF).contains(&cp) {
        CharType::Kanji
    } else if (0x0020..=0x007E).contains(&cp) {
        CharType::Ascii
    } else {
        CharType::Other
    }
}

pub fn tokenize_line(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut ascii_buffer = String::new();

    let flush_ascii = |buffer: &mut String, out: &mut Vec<String>| {
        if !buffer.is_empty() {
            out.push(std::mem::take(buffer));
        }
    };

    for c in line.chars() {
        if c.is_ascii() && c.is_ascii_alphanumeric() {
            ascii_buffer.push(c);
        } else if c == ' ' {
            flush_ascii(&mut ascii_buffer, &mut tokens);
        } else {
            flush_ascii(&mut ascii_buffer, &mut tokens);
            tokens.push(c.to_string());
        }
    }

    flush_ascii(&mut ascii_buffer, &mut tokens);
    tokens
}

pub fn add_feature_ctype(tokens: &[String]) -> Vec<String> {
    tokens
        .iter()
        .map(|token| {
            if token.chars().count() == 1
                && token
                    .chars()
                    .next()
                    .is_some_and(|c| char_type(c) == CharType::Hiragana)
            {
                "hira".to_string()
            } else {
                "non-hira".to_string()
            }
        })
        .collect()
}

pub fn add_feature_char(tokens: &[String]) -> Vec<String> {
    tokens.to_vec()
}

pub fn add_feature_char_left(tokens: &[String]) -> Vec<String> {
    if tokens.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::with_capacity(tokens.len());
    result.push("BOS".to_string());
    result.extend(tokens.iter().take(tokens.len() - 1).cloned());
    result
}

pub fn add_feature_char_right(tokens: &[String]) -> Vec<String> {
    if tokens.is_empty() {
        return Vec::new();
    }

    let mut result: Vec<String> = tokens.iter().skip(1).cloned().collect();
    result.push("EOS".to_string());
    result
}

pub fn add_feature_bigram_left(tokens: &[String]) -> Vec<String> {
    let left = add_feature_char_left(tokens);
    left.into_iter()
        .zip(tokens.iter())
        .map(|(l, t)| format!("{l} {t}"))
        .collect()
}

pub fn add_feature_bigram_right(tokens: &[String]) -> Vec<String> {
    let right = add_feature_char_right(tokens);
    tokens
        .iter()
        .zip(right)
        .map(|(t, r)| format!("{t} {r}"))
        .collect()
}

pub fn add_feature_trigram_left(tokens: &[String]) -> Vec<String> {
    match tokens.len() {
        0 => Vec::new(),
        1 => vec![format!("BOS BOS {}", tokens[0])],
        2 => vec![
            format!("BOS BOS {}", tokens[0]),
            format!("BOS {} {}", tokens[0], tokens[1]),
        ],
        _ => {
            let mut left2 = vec!["BOS".to_string(), "BOS".to_string()];
            left2.extend(tokens.iter().take(tokens.len() - 2).cloned());
            let mut left1 = vec!["BOS".to_string()];
            left1.extend(tokens.iter().take(tokens.len() - 1).cloned());

            left2
                .into_iter()
                .zip(left1)
                .zip(tokens.iter())
                .map(|((l2, l1), t)| format!("{l2} {l1} {t}"))
                .collect()
        }
    }
}

pub fn add_feature_trigram_right(tokens: &[String]) -> Vec<String> {
    match tokens.len() {
        0 => Vec::new(),
        1 => vec![format!("{} EOS EOS", tokens[0])],
        2 => vec![
            format!("{} {} EOS", tokens[0], tokens[1]),
            format!("{} EOS EOS", tokens[1]),
        ],
        _ => {
            let mut right1: Vec<String> = tokens.iter().skip(1).cloned().collect();
            right1.push("EOS".to_string());

            let mut right2: Vec<String> = tokens.iter().skip(2).cloned().collect();
            right2.push("EOS".to_string());
            right2.push("EOS".to_string());

            tokens
                .iter()
                .zip(right1)
                .zip(right2)
                .map(|((t, r1), r2)| format!("{t} {r1} {r2}"))
                .collect()
        }
    }
}

pub fn add_feature_dict_max_kl_start(
    tokens: &[String],
    materials: &CrfFeatureMaterials,
) -> Vec<String> {
    tokens
        .iter()
        .map(|token| {
            materials
                .max_key_len_starting_with
                .get(token)
                .copied()
                .unwrap_or(0)
                .to_string()
        })
        .collect()
}

pub fn add_feature_dict_max_kl_end(
    tokens: &[String],
    materials: &CrfFeatureMaterials,
) -> Vec<String> {
    tokens
        .iter()
        .map(|token| {
            materials
                .max_key_len_ending_with
                .get(token)
                .copied()
                .unwrap_or(0)
                .to_string()
        })
        .collect()
}

pub fn add_feature_dict_entry_ct_start(
    tokens: &[String],
    materials: &CrfFeatureMaterials,
) -> Vec<String> {
    tokens
        .iter()
        .map(|token| {
            log2_bucket(
                materials
                    .dict_entry_count_starting_with
                    .get(token)
                    .copied()
                    .unwrap_or(0),
            )
            .to_string()
        })
        .collect()
}

pub fn add_feature_dict_entry_ct_end(
    tokens: &[String],
    materials: &CrfFeatureMaterials,
) -> Vec<String> {
    tokens
        .iter()
        .map(|token| {
            log2_bucket(
                materials
                    .dict_entry_count_ending_with
                    .get(token)
                    .copied()
                    .unwrap_or(0),
            )
            .to_string()
        })
        .collect()
}

pub fn add_features_per_line(
    line_or_tokens: impl Into<LineOrTokens>,
    dict_materials: Option<&CrfFeatureMaterials>,
) -> Vec<FeatureRow> {
    let tokens = match line_or_tokens.into() {
        LineOrTokens::Line(line) => tokenize_line(&line),
        LineOrTokens::Tokens(tokens) => tokens,
    };

    let n = tokens.len();
    if n == 0 {
        return Vec::new();
    }

    let mut features = vec![HashMap::new(); n];
    merge_feature_column(&mut features, "char", add_feature_char(&tokens));
    merge_feature_column(&mut features, "char_left", add_feature_char_left(&tokens));
    merge_feature_column(&mut features, "char_right", add_feature_char_right(&tokens));
    merge_feature_column(&mut features, "bigram_left", add_feature_bigram_left(&tokens));
    merge_feature_column(&mut features, "bigram_right", add_feature_bigram_right(&tokens));
    merge_feature_column(&mut features, "trigram_left", add_feature_trigram_left(&tokens));
    merge_feature_column(&mut features, "trigram_right", add_feature_trigram_right(&tokens));
    merge_feature_column(&mut features, "ctype", add_feature_ctype(&tokens));

    if let Some(materials) = dict_materials {
        merge_feature_column(
            &mut features,
            "dict_max_kl_s",
            add_feature_dict_max_kl_start(&tokens, materials),
        );
        merge_feature_column(
            &mut features,
            "dict_max_kl_e",
            add_feature_dict_max_kl_end(&tokens, materials),
        );
        merge_feature_column(
            &mut features,
            "dict_entry_ct_s",
            add_feature_dict_entry_ct_start(&tokens, materials),
        );
        merge_feature_column(
            &mut features,
            "dict_entry_ct_e",
            add_feature_dict_entry_ct_end(&tokens, materials),
        );
    }

    features
}

pub fn crf_compute_emission_scores(
    features: &[FeatureRow],
    state_features: &StateFeatureWeights,
    labels: &[String],
) -> Vec<Vec<f64>> {
    let n_positions = features.len();
    let n_labels = labels.len();

    let mut label_to_idx = HashMap::new();
    for (idx, label) in labels.iter().enumerate() {
        label_to_idx.insert(label.clone(), idx);
    }

    let mut emission = vec![vec![0.0; n_labels]; n_positions];
    for (t, feature_row) in features.iter().enumerate() {
        for (key, value) in feature_row {
            let feat_str = format!("{key}:{value}");
            for label in labels {
                if let Some(weight) = state_features.get(&(feat_str.clone(), label.clone())) {
                    if *weight != 0.0 {
                        let idx = label_to_idx[label];
                        emission[t][idx] += weight;
                    }
                }
            }
        }
    }

    emission
}

pub fn crf_nbest_viterbi(
    emission: &[Vec<f64>],
    transitions: &TransitionWeights,
    labels: &[String],
    n_best: usize,
) -> Vec<NBestPath> {
    let n_positions = emission.len();
    let n_labels = labels.len();

    if n_positions == 0 || n_labels == 0 || n_best == 0 {
        return Vec::new();
    }

    let mut trans = vec![vec![0.0; n_labels]; n_labels];
    for (i, from_label) in labels.iter().enumerate() {
        for (j, to_label) in labels.iter().enumerate() {
            trans[i][j] = transitions
                .get(&(from_label.clone(), to_label.clone()))
                .copied()
                .unwrap_or(0.0);
        }
    }

    let mut dp: Vec<Vec<Vec<(f64, Option<(usize, usize)>)>>> =
        vec![vec![Vec::new(); n_labels]; n_positions];

    for label_idx in 0..n_labels {
        dp[0][label_idx].push((emission[0][label_idx], None));
    }

    for t in 1..n_positions {
        for curr_label in 0..n_labels {
            let mut candidates = Vec::new();
            for prev_label in 0..n_labels {
                for (rank, (prev_score, _)) in dp[t - 1][prev_label].iter().enumerate() {
                    let score = prev_score + trans[prev_label][curr_label] + emission[t][curr_label];
                    candidates.push((score, Some((prev_label, rank))));
                }
            }
            candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
            candidates.truncate(n_best);
            dp[t][curr_label] = candidates;
        }
    }

    let mut final_candidates = Vec::new();
    for label_idx in 0..n_labels {
        for (rank, (score, _)) in dp[n_positions - 1][label_idx].iter().enumerate() {
            final_candidates.push((*score, label_idx, rank));
        }
    }
    final_candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    final_candidates.truncate(n_best);

    let mut results = Vec::new();
    for (final_score, final_label, final_rank) in final_candidates {
        let mut path = vec![final_label];
        let mut curr_label = final_label;
        let mut curr_rank = final_rank;

        for t in (1..n_positions).rev() {
            let (_, backptr) = &dp[t][curr_label][curr_rank];
            let Some((prev_label, prev_rank)) = backptr else {
                break;
            };
            path.push(*prev_label);
            curr_label = *prev_label;
            curr_rank = *prev_rank;
        }

        path.reverse();
        results.push(NBestPath {
            labels: path.into_iter().map(|idx| labels[idx].clone()).collect(),
            score: final_score,
        });
    }

    results
}

pub fn labels_to_bunsetsu(tokens: &[String], labels: &[String]) -> Vec<BunsetsuSegment> {
    if tokens.is_empty() || labels.is_empty() {
        return Vec::new();
    }

    let mut bunsetsu_list = Vec::new();
    let mut current_bunsetsu: Vec<String> = Vec::new();
    let mut current_label: Option<String> = None;

    for (token, label) in tokens.iter().zip(labels.iter()) {
        if label.starts_with('B') || current_label.is_none() {
            if !current_bunsetsu.is_empty() {
                bunsetsu_list.push(BunsetsuSegment {
                    text: current_bunsetsu.concat(),
                    label: current_label.take().unwrap(),
                });
            }

            current_bunsetsu = vec![token.clone()];
            current_label = Some(if label.starts_with('I') {
                format!("B{}", &label[1..])
            } else {
                label.clone()
            });
        } else {
            current_bunsetsu.push(token.clone());
        }
    }

    if !current_bunsetsu.is_empty() {
        bunsetsu_list.push(BunsetsuSegment {
            text: current_bunsetsu.concat(),
            label: current_label.unwrap_or_default(),
        });
    }

    bunsetsu_list
}

pub fn get_package_name() -> &'static str {
    PACKAGE_NAME
}

pub fn get_version() -> &'static str {
    VERSION
}

pub fn get_datadir() -> PathBuf {
    env::var_os("PSKK_INSTALL_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_INSTALL_ROOT))
}

pub fn get_default_config_path() -> PathBuf {
    get_datadir().join("data/default_user_config.json")
}

pub fn get_localedir() -> PathBuf {
    PathBuf::from("/usr/share/locale")
}

pub fn get_homedir() -> Option<PathBuf> {
    dirs::home_dir()
}

pub fn get_user_config_dir() -> PathBuf {
    dirs::config_dir()
        .map(|p| p.join(PACKAGE_NAME))
        .unwrap_or_else(|| PathBuf::from(".").join(PACKAGE_NAME))
}

pub fn get_user_dictionaries_dir() -> PathBuf {
    get_user_config_dir().join("dictionaries")
}

pub fn get_crf_model_path() -> PathBuf {
    get_user_config_dir().join("bunsetsu.crfsuite")
}

pub fn read_json_value(path: &Path) -> Result<Value, UtilError> {
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn write_json_value(path: &Path, value: &Value) -> Result<(), UtilError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(value)?;
    fs::write(path, content)?;
    Ok(())
}

pub fn get_default_config_data() -> Result<Value, UtilError> {
    read_json_value(&get_default_config_path())
}

fn write_log(message: &str) -> Result<(), UtilError> {
    let log_path = get_user_config_dir().join("pskk.log");
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    
    // Get current timestamp in a simple readable format
    let now = std::time::SystemTime::now();
    let duration = now.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs();
    
    // Simple date/time formatting: YYYY-MM-DD HH:MM:SS
    let days_since_epoch = secs / 86400;
    let secs_today = secs % 86400;
    let hours = secs_today / 3600;
    let minutes = (secs_today % 3600) / 60;
    let seconds = secs_today % 60;
    
    // Approximate date (this is simplified, not accounting for leap years perfectly)
    let years_since_1970 = days_since_epoch / 365;
    let year = 1970 + years_since_1970;
    let day_of_year = days_since_epoch % 365;
    let month = (day_of_year / 30) + 1;
    let day = (day_of_year % 30) + 1;
    
    writeln!(
        file,
        "[{:04}-{:02}-{:02} {:02}:{:02}:{:02}] {}",
        year, month, day, hours, minutes, seconds, message
    )?;
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), UtilError> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }
    
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        
        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    
    Ok(())
}

fn initialize_user_config_files() -> Result<Vec<String>, UtilError> {
    let mut warnings = Vec::new();
    let user_config_dir = get_user_config_dir();
    let data_dir = get_datadir().join("data");
    
    // Copy kanchoku_layouts directory
    let kanchoku_src = data_dir.join("kanchoku_layouts");
    let kanchoku_dst = user_config_dir.join("kanchoku_layouts");
    if kanchoku_src.exists() {
        copy_dir_recursive(&kanchoku_src, &kanchoku_dst)?;
        warnings.push(format!(
            "Copied kanchoku_layouts from {} to {}",
            kanchoku_src.display(),
            kanchoku_dst.display()
        ));
    }
    
    // Copy layouts directory
    let layouts_src = data_dir.join("layouts");
    let layouts_dst = user_config_dir.join("layouts");
    if layouts_src.exists() {
        copy_dir_recursive(&layouts_src, &layouts_dst)?;
        warnings.push(format!(
            "Copied layouts from {} to {}",
            layouts_src.display(),
            layouts_dst.display()
        ));
    }
    
    // Copy bunsetsu.crfsuite file
    let crfsuite_src = data_dir.join("crf_training/bunsetsu.crfsuite");
    let crfsuite_dst = user_config_dir.join("bunsetsu.crfsuite");
    if crfsuite_src.exists() {
        fs::copy(&crfsuite_src, &crfsuite_dst)?;
        warnings.push(format!(
            "Copied bunsetsu.crfsuite from {} to {}",
            crfsuite_src.display(),
            crfsuite_dst.display()
        ));
    }
    
    // Create empty dictionaries directory for user SKK dictionaries
    let user_dicts_dir = get_user_dictionaries_dir();
    if !user_dicts_dir.exists() {
        fs::create_dir_all(&user_dicts_dir)?;
        warnings.push(format!(
            "Created user dictionaries directory at {}",
            user_dicts_dir.display()
        ));
    }
    
    // Create empty extended_dictionary.json if it doesn't exist
    let ext_dict_path = user_config_dir.join("extended_dictionary.json");
    if !ext_dict_path.exists() {
        let empty_dict = json!({});
        write_json_value(&ext_dict_path, &empty_dict)?;
        warnings.push(format!(
            "Created empty extended_dictionary.json at {}",
            ext_dict_path.display()
        ));
    }
    
    Ok(warnings)
}

pub fn get_config_data() -> Result<(Value, Vec<String>), UtilError> {
    let config_path = get_user_config_dir().join("config.json");
    let mut warnings = Vec::new();
    let default_config = get_default_config_data()?;

    if !config_path.exists() {
        fs::create_dir_all(get_user_config_dir())?;
        write_json_value(&config_path, &default_config)?;
        
        let msg = format!(
            "config.json not found under {}. Copied default config from {}",
            get_user_config_dir().display(),
            get_default_config_path().display()
        );
        let _ = write_log(&msg);
        warnings.push(msg);
        
        // Initialize user config directory with layouts and CRF model
        let mut init_warnings = initialize_user_config_files()?;
        for warning in &init_warnings {
            let _ = write_log(warning);
        }
        warnings.append(&mut init_warnings);
        
        return Ok((default_config, warnings));
    }

    let mut config_data = match read_json_value(&config_path) {
        Ok(value) => value,
        Err(UtilError::Json(_)) => default_config.clone(),
        Err(err) => return Err(err),
    };

    merge_default_config(&mut config_data, &default_config, &mut warnings);
    validate_dictionaries_config(&mut config_data, &default_config, &mut warnings);
    
    // Log all warnings
    for warning in &warnings {
        let _ = write_log(warning);
    }
    
    Ok((config_data, warnings))
}

pub fn save_config_data(config_data: &Value) -> Result<(), UtilError> {
    write_json_value(&get_user_config_dir().join("config.json"), config_data)
}

pub fn get_layout_data(config: &Value) -> Result<Value, UtilError> {
    let layout_name = config
        .get("layout")
        .and_then(Value::as_str)
        .ok_or(UtilError::MissingField("layout"))?;

    let user_path = get_user_config_dir().join("layouts").join(layout_name);
    let system_path = get_datadir().join("layouts").join(layout_name);
    let fallback_path = get_datadir().join("layouts").join("shingeta.json");

    let chosen = if user_path.exists() {
        user_path
    } else if system_path.exists() {
        system_path
    } else {
        fallback_path
    };

    read_json_value(&chosen)
}

pub fn get_kanchoku_layout(config: &Value) -> Result<Value, UtilError> {
    let layout_name = config
        .get("kanchoku_layout")
        .and_then(Value::as_str)
        .ok_or(UtilError::MissingField("kanchoku_layout"))?;

    let candidates = [
        get_user_config_dir()
            .join("kanchoku_layouts")
            .join(layout_name),
        get_user_config_dir().join(layout_name),
        get_datadir().join("kanchoku_layouts").join(layout_name),
        get_datadir().join("kanchoku_layouts").join("aki_code.json"),
    ];

    let chosen = candidates
        .iter()
        .find(|path| path.exists())
        .cloned()
        .unwrap_or_else(|| candidates.last().unwrap().clone());

    read_json_value(&chosen)
}

pub fn get_dictionary_files(config_dir: Option<&Path>) -> Vec<PathBuf> {
    let base = config_dir
        .map(PathBuf::from)
        .unwrap_or_else(get_user_config_dir);
    let names = [
        "system_dictionary.json",
        "imported_user_dictionary.json",
        "user_dictionary.json",
        "extended_dictionary.json",
    ];

    names
        .iter()
        .map(|name| base.join(name))
        .filter(|path| path.exists())
        .collect()
}

pub fn load_dictionary_json(path: &Path) -> Result<Dictionary, UtilError> {
    let value = read_json_value(path)?;
    dictionary_from_value(value)
}

pub fn write_dictionary_json(path: &Path, dictionary: &Dictionary) -> Result<(), UtilError> {
    write_json_value(path, &dictionary_to_value(dictionary))
}

pub fn load_and_merge_dictionary_files(paths: &[PathBuf]) -> Result<Dictionary, UtilError> {
    let mut merged = Dictionary::new();

    for path in paths {
        if !path.exists() {
            continue;
        }

        let dictionary = load_dictionary_json(path)?;
        for (reading, candidates) in dictionary {
            let merged_candidates = merged.entry(reading).or_default();
            for (candidate, count) in candidates {
                *merged_candidates.entry(candidate).or_insert(0) += count;
            }
        }
    }

    Ok(merged)
}

pub fn build_crf_feature_materials(merged_dictionary: &Dictionary) -> CrfFeatureMaterials {
    let mut materials = CrfFeatureMaterials::default();

    for (yomi, candidates) in merged_dictionary {
        if yomi.is_empty() {
            continue;
        }

        let yomi_len = yomi.chars().count();
        let num_entries = candidates.len();
        let first_char = yomi.chars().next().unwrap().to_string();
        let last_char = yomi.chars().last().unwrap().to_string();

        materials
            .max_key_len_starting_with
            .entry(first_char.clone())
            .and_modify(|len| *len = (*len).max(yomi_len))
            .or_insert(yomi_len);
        materials
            .max_key_len_ending_with
            .entry(last_char.clone())
            .and_modify(|len| *len = (*len).max(yomi_len))
            .or_insert(yomi_len);
        *materials
            .dict_entry_count_starting_with
            .entry(first_char)
            .or_insert(0) += num_entries;
        *materials
            .dict_entry_count_ending_with
            .entry(last_char)
            .or_insert(0) += num_entries;
    }

    materials
}

pub fn generate_crf_feature_materials(
    output_path: Option<&Path>,
) -> Result<(PathBuf, CrfFeatureMaterials), UtilError> {
    let path = output_path
        .map(PathBuf::from)
        .unwrap_or_else(|| get_user_config_dir().join("crf_feature_materials.json"));
    let merged = load_and_merge_dictionary_files(&get_dictionary_files(None))?;
    let materials = build_crf_feature_materials(&merged);
    write_json_value(&path, &serde_json::to_value(&materials)?)?;
    Ok((path, materials))
}

pub fn load_crf_feature_materials(path: Option<&Path>) -> Result<CrfFeatureMaterials, UtilError> {
    let path = path
        .map(PathBuf::from)
        .unwrap_or_else(|| get_user_config_dir().join("crf_feature_materials.json"));
    if !path.exists() {
        return Ok(CrfFeatureMaterials::default());
    }
    Ok(serde_json::from_value(read_json_value(&path)?)?)
}

pub fn parse_skk_dictionary_line(line: &str) -> Option<(String, Vec<String>)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with(';') {
        return None;
    }

    let (reading, candidates_part) = line.split_once(' ')?;
    let candidates_part = candidates_part.trim_matches('/');
    if candidates_part.is_empty() {
        return None;
    }

    let candidates: Vec<String> = candidates_part
        .split('/')
        .filter_map(|candidate| {
            let surface = candidate.split(';').next().unwrap_or_default();
            (!surface.is_empty()).then(|| surface.to_string())
        })
        .collect();

    if candidates.is_empty() {
        return None;
    }

    Some((reading.to_string(), candidates))
}

pub fn merge_weighted_skk_entries<'a, I>(
    entries: I,
    source_weight: i32,
    expand_okurigana: impl Fn(&str, &str, i32) -> Vec<(String, String, i32)>,
    is_skk_okurigana_entry: impl Fn(&str) -> bool,
) -> (Dictionary, DictionaryBuildStats)
where
    I: IntoIterator<Item = &'a str>,
{
    let mut merged_dictionary = Dictionary::new();
    let mut stats = DictionaryBuildStats::default();
    let mut seen_in_this_file: HashMap<String, HashSet<String>> = HashMap::new();

    for line in entries {
        let Some((reading, candidates)) = parse_skk_dictionary_line(line) else {
            continue;
        };

        if is_skk_okurigana_entry(&reading) {
            for candidate in candidates {
                let expanded = expand_okurigana(&reading, &candidate, source_weight);
                if expanded.is_empty() {
                    continue;
                }

                stats.okurigana_entries_expanded += 1;
                for (conj_reading, conj_surface, count) in expanded {
                    let seen = seen_in_this_file.entry(conj_reading.clone()).or_default();
                    if !seen.insert(conj_surface.clone()) {
                        continue;
                    }
                    *merged_dictionary
                        .entry(conj_reading)
                        .or_default()
                        .entry(conj_surface)
                        .or_insert(0) += count;
                }
            }
        } else {
            let seen = seen_in_this_file.entry(reading.clone()).or_default();
            for candidate in candidates {
                if !seen.insert(candidate.clone()) {
                    continue;
                }
                *merged_dictionary
                    .entry(reading.clone())
                    .or_default()
                    .entry(candidate)
                    .or_insert(0) += source_weight;
            }
        }
    }

    stats.files_processed = 1;
    stats.total_readings = merged_dictionary.len();
    stats.total_candidates = merged_dictionary.values().map(HashMap::len).sum();
    (merged_dictionary, stats)
}

pub fn generate_system_dictionary_from_skk_lines<'a, I>(
    entries: I,
    output_path: Option<&Path>,
    source_weight: i32,
    expand_okurigana: impl Fn(&str, &str, i32) -> Vec<(String, String, i32)>,
    is_skk_okurigana_entry: impl Fn(&str) -> bool,
) -> Result<(PathBuf, DictionaryBuildStats), UtilError>
where
    I: IntoIterator<Item = &'a str>,
{
    let path = output_path
        .map(PathBuf::from)
        .unwrap_or_else(|| get_user_config_dir().join("system_dictionary.json"));
    let (dictionary, stats) = merge_weighted_skk_entries(
        entries,
        source_weight,
        expand_okurigana,
        is_skk_okurigana_entry,
    );
    write_dictionary_json(&path, &dictionary)?;
    Ok((path, stats))
}

pub fn generate_user_dictionary_from_skk_lines<'a, I>(
    entries: I,
    output_path: Option<&Path>,
    source_weight: i32,
) -> Result<(PathBuf, DictionaryBuildStats), UtilError>
where
    I: IntoIterator<Item = &'a str>,
{
    let default_output = get_user_config_dir().join("imported_user_dictionary.json");
    let resolved_output = output_path
        .map(PathBuf::from)
        .unwrap_or(default_output);
    generate_system_dictionary_from_skk_lines(
        entries,
        Some(resolved_output.as_path()),
        source_weight,
        |_reading, _candidate, _weight| Vec::new(),
        |_reading| false,
    )
}

pub fn generate_system_dictionary_from_sources(
    source_weights: &HashMap<String, i32>,
) -> Result<(PathBuf, DictionaryBuildStats), UtilError> {
    let mut merged = Dictionary::new();
    let mut stats = DictionaryBuildStats::default();
    let output_path = get_user_config_dir().join("system_dictionary.json");

    for (file_path, weight) in source_weights {
        let lines = read_utf8_lines(Path::new(file_path))?;
        let (dictionary, file_stats) = merge_weighted_skk_entries(
            lines.iter().map(String::as_str),
            *weight,
            |reading, kanji, base_count| {
                crate::katsuyou::expand_skk_okurigana(reading, kanji, base_count)
            },
            crate::katsuyou::is_skk_okurigana_entry,
        );
        merge_dictionary_into(&mut merged, dictionary);
        stats.files_processed += usize::from(file_stats.files_processed > 0);
        stats.okurigana_entries_expanded += file_stats.okurigana_entries_expanded;
    }

    stats.total_readings = merged.len();
    stats.total_candidates = merged.values().map(HashMap::len).sum();
    write_dictionary_json(&output_path, &merged)?;
    Ok((output_path, stats))
}

pub fn generate_user_dictionary_from_sources(
    source_weights: &HashMap<String, i32>,
) -> Result<(Option<PathBuf>, DictionaryBuildStats), UtilError> {
    let mut merged = Dictionary::new();
    let mut stats = DictionaryBuildStats::default();

    if source_weights.is_empty() {
        return Ok((None, stats));
    }

    for (filename, weight) in source_weights {
        let path = get_user_dictionaries_dir().join(filename);
        let lines = read_utf8_lines(&path)?;
        let (dictionary, file_stats) = merge_weighted_skk_entries(
            lines.iter().map(String::as_str),
            *weight,
            |_reading, _kanji, _base_count| Vec::new(),
            |_reading| false,
        );
        merge_dictionary_into(&mut merged, dictionary);
        stats.files_processed += usize::from(file_stats.files_processed > 0);
    }

    stats.total_readings = merged.len();
    stats.total_candidates = merged.values().map(HashMap::len).sum();
    let output_path = get_user_config_dir().join("imported_user_dictionary.json");
    write_dictionary_json(&output_path, &merged)?;
    Ok((Some(output_path), stats))
}

pub fn generate_extended_dictionary(
    config: &Value,
    source_paths: &[String],
) -> Result<(PathBuf, ExtendedDictionaryStats), UtilError> {
    let mut stats = ExtendedDictionaryStats::default();
    let output_path = get_user_config_dir().join("extended_dictionary.json");
    let kanchoku_layout = get_kanchoku_layout(config)?;

    let mut kanchoku_kanji = HashSet::new();
    if let Some(root) = kanchoku_layout.as_object() {
        for second_value in root.values() {
            let Some(second_map) = second_value.as_object() else {
                continue;
            };
            for kanji in second_map.values().filter_map(Value::as_str) {
                kanchoku_kanji.insert(kanji.to_string());
            }
        }
    }
    stats.kanchoku_kanji_count = kanchoku_kanji.len();

    let mut yomi_to_kanji: HashMap<String, HashSet<String>> = HashMap::new();
    for file_path in source_paths {
        let path = Path::new(file_path);
        if !path.is_file() {
            continue;
        }

        let lines = read_utf8_lines(path)?;
        let mut processed = false;
        for line in &lines {
            let Some((reading, candidates)) = parse_skk_dictionary_line(line) else {
                continue;
            };
            for candidate in candidates {
                if candidate.chars().count() == 1 && kanchoku_kanji.contains(&candidate) {
                    yomi_to_kanji
                        .entry(reading.clone())
                        .or_default()
                        .insert(candidate);
                }
            }
            processed = true;
        }
        if processed {
            stats.files_processed += 1;
        }
    }
    stats.yomi_kanji_mappings = yomi_to_kanji.values().map(HashSet::len).sum();

    let config_dir = get_user_config_dir();
    let mut combined_dictionary = Dictionary::new();
    for dict_filename in ["system_dictionary.json", "imported_user_dictionary.json"] {
        let dict_path = config_dir.join(dict_filename);
        if !dict_path.exists() {
            continue;
        }
        let dictionary = load_dictionary_json(&dict_path)?;
        for (reading, candidates) in dictionary {
            let merged_candidates = combined_dictionary.entry(reading).or_default();
            for (candidate, count) in candidates {
                let current = merged_candidates.entry(candidate).or_insert(count);
                if count > *current {
                    *current = count;
                }
            }
        }
    }
    stats.source_entries_scanned = combined_dictionary.len();

    let extended_dictionary = build_extended_dictionary(&combined_dictionary, &yomi_to_kanji);
    stats.total_readings = extended_dictionary.len();
    stats.total_candidates = extended_dictionary.values().map(HashMap::len).sum();
    write_dictionary_json(&output_path, &extended_dictionary)?;
    Ok((output_path, stats))
}

pub enum LineOrTokens {
    Line(String),
    Tokens(Vec<String>),
}

impl From<&str> for LineOrTokens {
    fn from(value: &str) -> Self {
        Self::Line(value.to_string())
    }
}

impl From<String> for LineOrTokens {
    fn from(value: String) -> Self {
        Self::Line(value)
    }
}

impl From<Vec<String>> for LineOrTokens {
    fn from(value: Vec<String>) -> Self {
        Self::Tokens(value)
    }
}

fn merge_feature_column(features: &mut [FeatureRow], key: &str, values: Vec<String>) {
    for (row, value) in features.iter_mut().zip(values) {
        row.insert(key.to_string(), value);
    }
}

fn log2_bucket(value: usize) -> usize {
    let mut v = value + 1;
    let mut bucket = 0;
    while v > 1 {
        v >>= 1;
        bucket += 1;
    }
    bucket
}

fn merge_default_config(target: &mut Value, default_config: &Value, warnings: &mut Vec<String>) {
    let (Some(target_obj), Some(default_obj)) = (target.as_object_mut(), default_config.as_object())
    else {
        return;
    };

    for (key, default_value) in default_obj {
        match target_obj.get(key) {
            Some(existing) if same_json_type(existing, default_value) => {}
            Some(_) => {
                warnings.push(format!(
                    "type mismatch for key \"{key}\". Replaced with default value"
                ));
                target_obj.insert(key.clone(), default_value.clone());
            }
            None => {
                warnings.push(format!(
                    "missing key \"{key}\" in config.json. Added default value"
                ));
                target_obj.insert(key.clone(), default_value.clone());
            }
        }
    }
}

fn validate_dictionaries_config(target: &mut Value, default_config: &Value, warnings: &mut Vec<String>) {
    let Some(target_obj) = target.as_object_mut() else {
        return;
    };
    let default_dicts = default_config
        .get("dictionaries")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_else(|| {
            Map::from_iter([
                ("system".to_string(), Value::Object(Map::new())),
                ("user".to_string(), Value::Object(Map::new())),
            ])
        });

    let dicts_value = target_obj
        .entry("dictionaries".to_string())
        .or_insert_with(|| Value::Object(default_dicts.clone()));

    let Some(dicts_obj) = dicts_value.as_object_mut() else {
        *dicts_value = Value::Object(default_dicts);
        warnings.push("invalid dictionaries config. Reset to default".to_string());
        return;
    };

    for key in ["system", "user"] {
        match dicts_obj.get(key) {
            Some(Value::Object(_)) | Some(Value::Array(_)) => {}
            Some(_) => {
                warnings.push(format!(
                    "dictionaries.{key} has invalid type. Reset to default"
                ));
                if let Some(default_value) = default_config
                    .get("dictionaries")
                    .and_then(Value::as_object)
                    .and_then(|obj| obj.get(key))
                {
                    dicts_obj.insert(key.to_string(), default_value.clone());
                }
            }
            None => {
                warnings.push(format!(
                    "dictionaries.{key} missing from config. Added default"
                ));
                if let Some(default_value) = default_config
                    .get("dictionaries")
                    .and_then(Value::as_object)
                    .and_then(|obj| obj.get(key))
                {
                    dicts_obj.insert(key.to_string(), default_value.clone());
                }
            }
        }
    }
}

fn same_json_type(left: &Value, right: &Value) -> bool {
    matches!(
        (left, right),
        (Value::Null, Value::Null)
            | (Value::Bool(_), Value::Bool(_))
            | (Value::Number(_), Value::Number(_))
            | (Value::String(_), Value::String(_))
            | (Value::Array(_), Value::Array(_))
            | (Value::Object(_), Value::Object(_))
    )
}

fn dictionary_to_value(dictionary: &Dictionary) -> Value {
    let mut root = Map::new();
    for (reading, candidates) in dictionary {
        let mut candidate_map = Map::new();
        for (candidate, count) in candidates {
            candidate_map.insert(candidate.clone(), Value::from(*count));
        }
        root.insert(reading.clone(), Value::Object(candidate_map));
    }
    Value::Object(root)
}

fn dictionary_from_value(value: Value) -> Result<Dictionary, UtilError> {
    let root = value
        .as_object()
        .ok_or(UtilError::InvalidConfig("dictionary JSON must be an object"))?;
    let mut dictionary = Dictionary::new();

    for (reading, candidates_value) in root {
        let candidates_obj = candidates_value.as_object().ok_or(UtilError::InvalidConfig(
            "dictionary entry candidates must be a JSON object",
        ))?;
        let mut candidates = CandidateCounts::new();
        for (candidate, count_value) in candidates_obj {
            let count = match count_value {
                Value::Number(number) => number.as_i64().unwrap_or(1) as i32,
                Value::Object(legacy) => legacy
                    .get("cost")
                    .and_then(Value::as_i64)
                    .map(|cost| -(cost as i32))
                    .unwrap_or(1),
                _ => 1,
            };
            candidates.insert(candidate.clone(), count);
        }
        dictionary.insert(reading.clone(), candidates);
    }

    Ok(dictionary)
}

fn read_utf8_lines(path: &Path) -> Result<Vec<String>, UtilError> {
    let content = fs::read_to_string(path)?;
    Ok(content.lines().map(|line| line.to_string()).collect())
}

fn find_substring_positions(haystack: &str, needle: &str) -> Vec<usize> {
    if needle.is_empty() {
        return Vec::new();
    }

    let mut positions = Vec::new();
    let mut start = 0;
    while start < haystack.len() {
        let Some(pos) = haystack[start..].find(needle) else {
            break;
        };
        let absolute_pos = start + pos;
        positions.push(absolute_pos);

        let advance = haystack[absolute_pos..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or(1);
        start = absolute_pos + advance;
    }
    positions
}

fn build_extended_dictionary(
    combined_dictionary: &Dictionary,
    yomi_to_kanji: &HashMap<String, HashSet<String>>,
) -> Dictionary {
    let mut extended_dictionary = Dictionary::new();

    for (reading, candidates) in combined_dictionary {
        for (yomi, kanji_set) in yomi_to_kanji {
            let positions = find_substring_positions(reading, yomi);
            if positions.is_empty() {
                continue;
            }

            for pos in positions {
                for kanji in kanji_set {
                    let matching_candidates: Vec<(&String, &i32)> = candidates
                        .iter()
                        .filter(|(candidate, _count)| {
                            candidate.contains(kanji) && candidate.chars().count() > 1
                        })
                        .collect();
                    if matching_candidates.is_empty() {
                        continue;
                    }

                    let new_reading = format!(
                        "{}{}{}",
                        &reading[..pos],
                        kanji,
                        &reading[pos + yomi.len()..]
                    );
                    let target_candidates = extended_dictionary.entry(new_reading).or_default();
                    for (candidate, count) in matching_candidates {
                        let current = target_candidates.entry(candidate.clone()).or_insert(*count);
                        if *count > *current {
                            *current = *count;
                        }
                    }
                }
            }
        }
    }

    extended_dictionary
}

fn merge_dictionary_into(target: &mut Dictionary, source: Dictionary) {
    for (reading, candidates) in source {
        let target_candidates = target.entry(reading).or_default();
        for (candidate, count) in candidates {
            *target_candidates.entry(candidate).or_insert(0) += count;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn labels(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    fn tokens(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn classifies_characters() {
        assert_eq!(char_type('あ'), CharType::Hiragana);
        assert_eq!(char_type('ア'), CharType::Katakana);
        assert_eq!(char_type('愛'), CharType::Kanji);
        assert_eq!(char_type('A'), CharType::Ascii);
        assert_eq!(char_type('€'), CharType::Other);
    }

    #[test]
    fn tokenizes_mixed_text() {
        assert_eq!(
            tokenize_line("きょうは sunny day"),
            tokens(&["き", "ょ", "う", "は", "sunny", "day"])
        );
        assert_eq!(
            tokenize_line("今日は2024年"),
            tokens(&["今", "日", "は", "2024", "年"])
        );
    }

    #[test]
    fn builds_context_features() {
        let input = tokens(&["あ", "い", "う"]);
        assert_eq!(add_feature_char_left(&input), tokens(&["BOS", "あ", "い"]));
        assert_eq!(add_feature_char_right(&input), tokens(&["い", "う", "EOS"]));
        assert_eq!(
            add_feature_bigram_left(&input),
            tokens(&["BOS あ", "あ い", "い う"])
        );
        assert_eq!(
            add_feature_trigram_right(&input),
            tokens(&["あ い う", "い う EOS", "う EOS EOS"])
        );
    }

    #[test]
    fn combines_features_with_materials() {
        let mut materials = CrfFeatureMaterials::default();
        materials
            .max_key_len_starting_with
            .insert("き".to_string(), 6);
        materials
            .dict_entry_count_ending_with
            .insert("き".to_string(), 7);

        let features = add_features_per_line("き", Some(&materials));
        assert_eq!(features.len(), 1);
        assert_eq!(features[0]["char"], "き");
        assert_eq!(features[0]["ctype"], "hira");
        assert_eq!(features[0]["dict_max_kl_s"], "6");
        assert_eq!(features[0]["dict_entry_ct_e"], "3");
    }

    #[test]
    fn computes_emission_scores() {
        let mut row = FeatureRow::new();
        row.insert("char".to_string(), "き".to_string());
        let mut state_features = StateFeatureWeights::new();
        state_features.insert(("char:き".to_string(), "B-L".to_string()), 1.5);
        state_features.insert(("char:き".to_string(), "I-L".to_string()), -0.5);

        let emission = crf_compute_emission_scores(&[row], &state_features, &labels(&["B-L", "I-L"]));
        assert_eq!(emission, vec![vec![1.5, -0.5]]);
    }

    #[test]
    fn viterbi_returns_best_paths() {
        let emission = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        let mut transitions = TransitionWeights::new();
        transitions.insert(("B-L".to_string(), "I-L".to_string()), 0.5);
        transitions.insert(("I-L".to_string(), "I-L".to_string()), 0.1);

        let results = crf_nbest_viterbi(&emission, &transitions, &labels(&["B-L", "I-L"]), 2);
        assert_eq!(results[0].labels, labels(&["B-L", "I-L"]));
    }

    #[test]
    fn groups_labels_into_bunsetsu() {
        let segments = labels_to_bunsetsu(
            &tokens(&["き", "ょ", "う", "は"]),
            &labels(&["B-L", "I-L", "I-L", "B-P"]),
        );
        assert_eq!(
            segments,
            vec![
                BunsetsuSegment {
                    text: "きょう".to_string(),
                    label: "B-L".to_string(),
                },
                BunsetsuSegment {
                    text: "は".to_string(),
                    label: "B-P".to_string(),
                },
            ]
        );
    }

    #[test]
    fn parses_skk_lines() {
        assert_eq!(
            parse_skk_dictionary_line("あやこ /亜矢子;人名/彩子;人名/"),
            Some((
                "あやこ".to_string(),
                tokens(&["亜矢子", "彩子"]),
            ))
        );
        assert_eq!(parse_skk_dictionary_line("; comment"), None);
    }

    #[test]
    fn builds_crf_materials_from_dictionary() {
        let mut dictionary = Dictionary::new();
        dictionary.insert(
            "きょう".to_string(),
            HashMap::from([("今日".to_string(), 1), ("京".to_string(), 1)]),
        );
        dictionary.insert(
            "きぎょう".to_string(),
            HashMap::from([("企業".to_string(), 1)]),
        );

        let materials = build_crf_feature_materials(&dictionary);
        assert_eq!(materials.max_key_len_starting_with["き"], 4);
        assert_eq!(materials.max_key_len_ending_with["う"], 4);
        assert_eq!(materials.dict_entry_count_starting_with["き"], 3);
    }

    #[test]
    fn merges_weighted_entries_with_okurigana_expansion() {
        let lines = vec!["かk /書/", "あい /愛/"];
        let (dictionary, stats) = merge_weighted_skk_entries(
            lines,
            2,
            |reading, kanji, weight| {
                if reading == "かk" && kanji == "書" {
                    vec![
                        ("かく".to_string(), "書く".to_string(), weight),
                        ("かいて".to_string(), "書いて".to_string(), weight),
                    ]
                } else {
                    Vec::new()
                }
            },
            |reading| reading.ends_with('k'),
        );

        assert_eq!(dictionary["あい"]["愛"], 2);
        assert_eq!(dictionary["かく"]["書く"], 2);
        assert_eq!(stats.okurigana_entries_expanded, 1);
    }

    #[test]
    fn finds_overlapping_substring_positions() {
        assert_eq!(find_substring_positions("あああ", "ああ"), vec![0, 3]);
    }

    #[test]
    fn builds_extended_dictionary_entries() {
        let combined = HashMap::from([(
            "きぎょう".to_string(),
            HashMap::from([
                ("企業".to_string(), 3),
                ("起業".to_string(), 2),
                ("企".to_string(), 10),
            ]),
        )]);
        let yomi_to_kanji = HashMap::from([(
            "き".to_string(),
            HashSet::from(["企".to_string()]),
        )]);

        let generated = build_extended_dictionary(&combined, &yomi_to_kanji);
        assert_eq!(generated["企ぎょう"]["企業"], 3);
        assert!(!generated["企ぎょう"].contains_key("起業"));
        assert!(!generated["企ぎょう"].contains_key("企"));
    }

    #[test]
    fn normalizes_config_against_defaults() {
        let mut config = json!({
            "layout": 1,
            "dictionaries": {
                "system": true
            }
        });
        let default_config = json!({
            "layout": "shingeta.json",
            "kanchoku_layout": "aki_code.json",
            "dictionaries": {
                "system": {},
                "user": {}
            }
        });
        let mut warnings = Vec::new();

        merge_default_config(&mut config, &default_config, &mut warnings);
        validate_dictionaries_config(&mut config, &default_config, &mut warnings);

        assert_eq!(config["layout"], json!("shingeta.json"));
        assert_eq!(config["kanchoku_layout"], json!("aki_code.json"));
        assert!(config["dictionaries"]["system"].is_object());
        assert!(config["dictionaries"]["user"].is_object());
        assert!(!warnings.is_empty());
    }

    #[test]
    fn parses_dictionary_json_with_legacy_cost_entries() {
        let value = json!({
            "あい": {
                "愛": 3,
                "藍": { "cost": -5 }
            }
        });

        let dictionary = dictionary_from_value(value).unwrap();
        assert_eq!(dictionary["あい"]["愛"], 3);
        assert_eq!(dictionary["あい"]["藍"], 5);
    }
}
