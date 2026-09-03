//! Prefix/suffix pass-through dictionary for partial-match conversion.
//!
//! Entries are hiragana fragments (particles, auxiliaries, honorific prefixes)
//! that should stay in kana while the rest of the yomi converts to kanji, e.g.
//! お + みせ(店) -> お店, みせ(店) + です -> 店です.
//!
//! The dictionary file format (e.g. `~/.config/pskk/pass_through_dictionary.json`):
//!
//! ```json
//! {
//!   "prefix": { "お": 10, "ご": 10 },
//!   "suffix": { "です": 8, "ます": 8, "に": 5 }
//! }
//! ```

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// Minimum length (in kana characters) of the middle lexical chunk.
/// Shorter mids (e.g. お + い -> お井) are pure noise and are excluded.
const MIN_MID_LEN: usize = 2;

#[derive(Debug, Default, Clone)]
pub struct PassThroughDictionary {
    pub prefix: HashMap<String, f64>,
    pub suffix: HashMap<String, f64>,
}

/// One valid split of a yomi into pass-through prefix / lexical mid /
/// pass-through suffix. Either side may be absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decomposition {
    pub prefix: Option<String>,
    pub mid: String,
    pub suffix: Option<String>,
}

impl PassThroughDictionary {
    /// Load from a JSON file. A missing or malformed file yields an empty
    /// dictionary (no pass-through candidates), never an error.
    pub fn load(path: &Path) -> Self {
        let Ok(content) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let Ok(raw) = serde_json::from_str::<RawDictionary>(&content) else {
            return Self::default();
        };
        Self {
            prefix: raw.prefix.unwrap_or_default(),
            suffix: raw.suffix.unwrap_or_default(),
        }
    }

    /// Enumerate every (prefix, mid, suffix) decomposition of `yomi` where the
    /// prefix is a pass-through prefix entry, the suffix is a pass-through
    /// suffix entry (either side may be absent), prefix and suffix do not
    /// overlap, and the middle is a non-empty chunk of at least `MIN_MID_LEN`
    /// kana. All combinations are tried; the caller scores and ranks them.
    pub fn decompose(&self, yomi: &str) -> Vec<Decomposition> {
        let yomi_len = yomi.chars().count();
        let mut out = Vec::new();

        for (p, _) in &self.prefix {
            let p_len = p.chars().count();
            if p.is_empty() || p_len >= yomi_len || !yomi.starts_with(p.as_str()) {
                continue;
            }
            // prefix + suffix
            for (s, _) in &self.suffix {
                let s_len = s.chars().count();
                if s.is_empty() || p_len + s_len >= yomi_len || !yomi.ends_with(s.as_str()) {
                    continue;
                }
                let mid = mid_chunk(yomi, p_len, s_len);
                if mid.chars().count() >= MIN_MID_LEN {
                    out.push(Decomposition {
                        prefix: Some(p.clone()),
                        mid,
                        suffix: Some(s.clone()),
                    });
                }
            }
            // prefix only
            let mid = mid_chunk(yomi, p_len, 0);
            if mid.chars().count() >= MIN_MID_LEN {
                out.push(Decomposition {
                    prefix: Some(p.clone()),
                    mid,
                    suffix: None,
                });
            }
        }

        // suffix only
        for (s, _) in &self.suffix {
            let s_len = s.chars().count();
            if s.is_empty() || s_len >= yomi_len || !yomi.ends_with(s.as_str()) {
                continue;
            }
            let mid = mid_chunk(yomi, 0, s_len);
            if mid.chars().count() >= MIN_MID_LEN {
                out.push(Decomposition {
                    prefix: None,
                    mid,
                    suffix: Some(s.clone()),
                });
            }
        }

        out
    }
}

fn mid_chunk(yomi: &str, prefix_len: usize, suffix_len: usize) -> String {
    yomi.chars()
        .skip(prefix_len)
        .take(yomi.chars().count() - prefix_len - suffix_len)
        .collect()
}

#[derive(Deserialize)]
struct RawDictionary {
    prefix: Option<HashMap<String, f64>>,
    suffix: Option<HashMap<String, f64>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PassThroughDictionary {
        let mut prefix = HashMap::new();
        prefix.insert("お".to_string(), 10.0);
        prefix.insert("ご".to_string(), 10.0);
        let mut suffix = HashMap::new();
        suffix.insert("です".to_string(), 8.0);
        suffix.insert("ます".to_string(), 8.0);
        suffix.insert("に".to_string(), 5.0);
        PassThroughDictionary { prefix, suffix }
    }

    #[test]
    fn prefix_only_decomposition() {
        let out = sample().decompose("おみせ");
        assert_eq!(
            out,
            vec![Decomposition {
                prefix: Some("お".to_string()),
                mid: "みせ".to_string(),
                suffix: None,
            }]
        );
    }

    #[test]
    fn suffix_only_decomposition() {
        let out = sample().decompose("みせです");
        assert_eq!(
            out,
            vec![Decomposition {
                prefix: None,
                mid: "みせ".to_string(),
                suffix: Some("です".to_string()),
            }]
        );
    }

    #[test]
    fn prefix_and_suffix_decomposition() {
        let out = sample().decompose("おみせです");
        // All combinations are tried: full combo plus single-sided decompositions
        assert_eq!(out.len(), 3);
        assert!(out.contains(&Decomposition {
            prefix: Some("お".to_string()),
            mid: "みせ".to_string(),
            suffix: Some("です".to_string()),
        }));
        assert!(out.contains(&Decomposition {
            prefix: Some("お".to_string()),
            mid: "みせです".to_string(),
            suffix: None,
        }));
        assert!(out.contains(&Decomposition {
            prefix: None,
            mid: "おみせ".to_string(),
            suffix: Some("です".to_string()),
        }));
    }

    #[test]
    fn short_mid_is_excluded() {
        // お + い: mid is only 1 kana -> excluded
        assert!(sample().decompose("おい").is_empty());
    }

    #[test]
    fn overlapping_prefix_and_suffix_excluded() {
        // おです: prefix お + suffix です would cover the whole yomi -> the
        // combined decomposition is excluded, but prefix-only (お | です) remains
        let out = sample().decompose("おです");
        assert!(!out.iter().any(|d| d.prefix.is_some() && d.suffix.is_some()));
        assert_eq!(
            out,
            vec![Decomposition {
                prefix: Some("お".to_string()),
                mid: "です".to_string(),
                suffix: None,
            }]
        );
    }

    #[test]
    fn empty_dictionary_decomposes_nothing() {
        assert!(PassThroughDictionary::default().decompose("おみせ").is_empty());
    }

    #[test]
    fn load_missing_file_is_empty() {
        let dict = PassThroughDictionary::load(Path::new("/nonexistent/pass_through_dictionary.json"));
        assert!(dict.prefix.is_empty());
        assert!(dict.suffix.is_empty());
    }
}
