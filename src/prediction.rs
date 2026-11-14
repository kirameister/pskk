//! Prefix prediction over the dictionary key space.
//!
//! The suggestion list deliberately treats the preedit text as a literal key
//! prefix: hiragana preedits match hiragana keys, katakana preedits match
//! katakana keys, and so on. No script normalisation happens here; that is the
//! dictionary maintainer's responsibility.
//!
//! Ranking is a small, deterministic scoring function so it can be exposed to
//! users later:
//!
//! ```text
//! score = exact_weight  * (key == query)
//!       + count_weight  * count
//!       - key_length_weight * (key_char_len - query_char_len)
//! ```
//!
//! Exact matches are meant to come first by default (a large `exact` weight),
//! then shorter completions, then higher counts. Ties are broken by reading
//! and surface so a `HashMap`-backed dictionary produces stable output.

use std::collections::HashMap;

use serde_json::Value;

use crate::henkan::Candidate;
use crate::util::Dictionary;

/// Default number of suggestions shown in one page. The IMF candidate window
/// currently renders nine entries (IBus lookup-table page size).
pub const DEFAULT_LIMIT: usize = 9;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PredictionWeights {
    pub exact: f64,
    pub count: f64,
    pub key_length: f64,
}

impl Default for PredictionWeights {
    fn default() -> Self {
        Self {
            exact: 1_000_000.0,
            count: 1.0,
            key_length: 1.0,
        }
    }
}

impl PredictionWeights {
    /// Read `prediction_rank_weights` from the engine configuration. Missing or
    /// invalid entries fall back to the defaults, so an old config keeps the
    /// agreed behaviour.
    pub fn from_config(config: &Value) -> Self {
        let defaults = Self::default();
        let Some(weights) = config.get("prediction_rank_weights") else {
            return defaults;
        };

        let read = |name: &str, fallback: f64| {
            weights
                .get(name)
                .and_then(Value::as_f64)
                .filter(|value| value.is_finite() && *value >= 0.0)
                .unwrap_or(fallback)
        };

        Self {
            exact: read("exact", defaults.exact),
            count: read("count", defaults.count),
            key_length: read("key_length", defaults.key_length),
        }
    }
}

/// One deduplicated candidate while ranking. Keeping the key lets us break
/// score ties deterministically and build the final `Candidate::reading`.
struct Ranked {
    surface: String,
    reading: String,
    count: i32,
    score: f64,
}

/// Find dictionary entries whose *key* starts with `query` and rank them.
///
/// `keys` must be sorted and contain every key of `dictionary`; the caller
/// (typically `HenkanProcessor`) owns that index so this function stays pure
/// and easy to test.
pub fn predict_prefix(
    dictionary: &Dictionary,
    keys: &[String],
    query: &str,
    limit: usize,
    weights: &PredictionWeights,
) -> Vec<Candidate> {
    if query.is_empty() || limit == 0 || keys.is_empty() {
        return Vec::new();
    }

    let query_len = query.chars().count();
    let start = keys.partition_point(|key| key.as_str() < query);

    // Deduplicate by surface: the same word can be reachable through several
    // readings (exact match plus longer completions), and only the best-scored
    // route should appear once.
    let mut best: HashMap<String, Ranked> = HashMap::new();

    for key in &keys[start..] {
        if !key.starts_with(query) {
            break;
        }
        let Some(candidates) = dictionary.get(key) else {
            continue;
        };

        let extra_len = key.chars().count().saturating_sub(query_len) as f64;
        let exact_bonus = if key == query { weights.exact } else { 0.0 };

        for (surface, count) in candidates {
            let score =
                exact_bonus + weights.count * (*count as f64) - weights.key_length * extra_len;

            match best.get_mut(surface) {
                Some(current) if is_better(score, key, &current.score, &current.reading) => {
                    current.reading = key.clone();
                    current.count = *count;
                    current.score = score;
                }
                Some(_) => {}
                None => {
                    best.insert(
                        surface.clone(),
                        Ranked {
                            surface: surface.clone(),
                            reading: key.clone(),
                            count: *count,
                            score,
                        },
                    );
                }
            }
        }
    }

    let mut ranked: Vec<Ranked> = best.into_values().collect();
    ranked.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.reading.cmp(&b.reading))
            .then_with(|| a.surface.cmp(&b.surface))
    });
    ranked.truncate(limit);

    ranked
        .into_iter()
        .map(|ranked| {
            let mut candidate = Candidate::new(ranked.surface, ranked.reading, ranked.count);
            candidate.score = ranked.score;
            candidate
        })
        .collect()
}

/// Score comparison with a deterministic tie-break: on equal scores prefer the
/// shorter reading, then the lexicographically smaller one.
fn is_better(score: f64, key: &str, current_score: &f64, current_key: &str) -> bool {
    match score.total_cmp(current_score) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => {
            (key.chars().count(), key) < (current_key.chars().count(), current_key)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn dict(entries: &[(&str, &[(&str, i32)])]) -> Dictionary {
        let mut dictionary = Dictionary::new();
        for (reading, candidates) in entries {
            let mut map = HashMap::new();
            for (surface, count) in *candidates {
                map.insert((*surface).to_string(), *count);
            }
            dictionary.insert((*reading).to_string(), map);
        }
        dictionary
    }

    fn keys(dictionary: &Dictionary) -> Vec<String> {
        let mut keys: Vec<String> = dictionary.keys().cloned().collect();
        keys.sort_unstable();
        keys
    }

    fn surfaces(candidates: &[Candidate]) -> Vec<&str> {
        candidates.iter().map(|c| c.surface.as_str()).collect()
    }

    #[test]
    fn exact_matches_come_before_completions() {
        let dictionary = dict(&[
            ("あい", &[("愛", 10), ("相", 5)]),
            ("あいさつ", &[("挨拶", 50)]),
            ("あいこく", &[("愛国", 40)]),
        ]);
        let result = predict_prefix(
            &dictionary,
            &keys(&dictionary),
            "あい",
            DEFAULT_LIMIT,
            &PredictionWeights::default(),
        );
        assert_eq!(surfaces(&result)[..2], ["愛", "相"]);
        assert!(surfaces(&result).contains(&"挨拶"));
    }

    #[test]
    fn prefixes_are_ranked_by_count_then_length() {
        let dictionary = dict(&[
            ("あい", &[("愛", 1)]),
            ("あいさつ", &[("挨拶", 10)]),
            ("あいさつまわり", &[("挨拶回り", 30)]),
        ]);
        let weights = PredictionWeights {
            exact: 0.0,
            count: 1.0,
            key_length: 0.0,
        };
        let result = predict_prefix(
            &dictionary,
            &keys(&dictionary),
            "あい",
            DEFAULT_LIMIT,
            &weights,
        );
        assert_eq!(surfaces(&result), ["挨拶回り", "挨拶", "愛"]);
    }

    #[test]
    fn shorter_completion_wins_when_counts_tie() {
        let dictionary = dict(&[
            ("あい", &[("愛", 1)]),
            ("あいさつ", &[("挨拶", 1)]),
            ("あいさ", &[("愛さ", 1)]),
        ]);
        let result = predict_prefix(
            &dictionary,
            &keys(&dictionary),
            "あい",
            DEFAULT_LIMIT,
            &PredictionWeights::default(),
        );
        // Exact "あい" first; then the +1 completion "あいさ"; then +2.
        assert_eq!(surfaces(&result), ["愛", "愛さ", "挨拶"]);
    }

    #[test]
    fn literal_script_is_not_normalised() {
        let dictionary = dict(&[
            ("あい", &[("愛", 1)]),
            ("アイ", &[("アイ", 1)]),
            ("a", &[("α", 1)]),
            ("aibo", &[("アイボ", 1)]),
            ("愛", &[("愛子", 1)]),
        ]);
        let keys = keys(&dictionary);
        let weights = PredictionWeights::default();

        let kata = predict_prefix(&dictionary, &keys, "アイ", DEFAULT_LIMIT, &weights);
        assert_eq!(surfaces(&kata), ["アイ"]);

        let ascii = predict_prefix(&dictionary, &keys, "a", DEFAULT_LIMIT, &weights);
        assert_eq!(surfaces(&ascii), ["α", "アイボ"]);

        let kanji = predict_prefix(&dictionary, &keys, "愛", DEFAULT_LIMIT, &weights);
        assert_eq!(surfaces(&kanji), ["愛子"]);
    }

    #[test]
    fn duplicates_keep_the_best_route() {
        let dictionary = dict(&[("あい", &[("愛", 10)]), ("あいさつ", &[("愛", 100)])]);
        let result = predict_prefix(
            &dictionary,
            &keys(&dictionary),
            "あい",
            DEFAULT_LIMIT,
            &PredictionWeights::default(),
        );
        assert_eq!(surfaces(&result), ["愛"]);
        assert_eq!(result[0].reading, "あい");
    }

    #[test]
    fn limit_is_respected() {
        let dictionary = dict(&[("あい", &[("a", 1), ("b", 1), ("c", 1), ("d", 1)])]);
        let result = predict_prefix(
            &dictionary,
            &keys(&dictionary),
            "あい",
            2,
            &PredictionWeights::default(),
        );
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn no_match_returns_empty() {
        let dictionary = dict(&[("あい", &[("愛", 1)])]);
        assert!(predict_prefix(
            &dictionary,
            &keys(&dictionary),
            "か",
            DEFAULT_LIMIT,
            &PredictionWeights::default()
        )
        .is_empty());
        assert!(predict_prefix(
            &dictionary,
            &keys(&dictionary),
            "",
            DEFAULT_LIMIT,
            &PredictionWeights::default()
        )
        .is_empty());
    }

    #[test]
    fn config_weights_override_defaults() {
        let config = serde_json::json!({
            "prediction_rank_weights": {
                "exact": 0.0,
                "count": 10.0,
                "key_length": 2.0
            }
        });
        let weights = PredictionWeights::from_config(&config);
        assert_eq!(weights.exact, 0.0);
        assert_eq!(weights.count, 10.0);
        assert_eq!(weights.key_length, 2.0);
    }
}
