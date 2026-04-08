use std::collections::{HashMap, HashSet};
use std::env;
use std::path::{Path, PathBuf};

pub const PACKAGE_NAME: &str = "ibus-pskk";
pub const VERSION: &str = "0.0.1";
pub const DEFAULT_INSTALL_ROOT: &str = "/opt/ibus-pskk";

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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
    get_datadir().join("config.json")
}

pub fn get_localedir() -> PathBuf {
    PathBuf::from("/usr/share/locale")
}

pub fn get_homedir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

pub fn get_user_config_dir() -> PathBuf {
    if let Some(config_home) = env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(config_home).join(PACKAGE_NAME);
    }

    get_homedir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join(PACKAGE_NAME)
}

pub fn get_user_dictionaries_dir() -> PathBuf {
    get_user_config_dir().join("dictionaries")
}

pub fn get_crf_model_path() -> PathBuf {
    get_user_config_dir().join("bunsetsu.crfsuite")
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
