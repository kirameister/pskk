//! Loading of trained CRFsuite models for bunsetsu segmentation.
//!
//! The IME runtime and the CRF trainer app share this parser: it decodes a
//! `.crfsuite` model file into the label list plus the state/transition weight
//! maps that the CRF inference in [`crate::util`] consumes.

use std::collections::HashMap;
use std::path::Path;

use crfsuite_compliant_rs::model::ModelReader;

use crate::util::{StateFeatureWeights, TransitionWeights};

/// Feature kinds used by the model's feature tables.
const FEATURE_KIND_STATE: u32 = 0;
const FEATURE_KIND_TRANSITION: u32 = 1;

/// A trained CRF model decoded into the maps used at inference time.
#[derive(Debug, Clone, Default)]
pub struct CrfWeights {
    /// Label names in model order (e.g. `B-L`, `I-L`, `B-P`, `I-P`).
    pub labels: Vec<String>,
    /// Emission weights: `(attribute, label) -> weight`.
    pub state_features: StateFeatureWeights,
    /// Transition weights: `(from_label, to_label) -> weight`.
    pub transitions: TransitionWeights,
}

impl CrfWeights {
    /// Decode a model from its raw bytes. Returns `None` when the bytes are not
    /// a CRFsuite model.
    ///
    /// Enumerates the model the same way the crate's own `dump` does: state
    /// features through the per-attribute refs, transitions through the
    /// per-label refs (the header's feature count is always 0).
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let model = ModelReader::open(bytes)?;

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

        Some(Self {
            labels,
            state_features,
            transitions,
        })
    }

    /// Read and decode a model file. Returns `None` when the file is missing,
    /// unreadable, or not a CRFsuite model — the IME then simply runs without
    /// CRF bunsetsu prediction.
    pub fn load(path: &Path) -> Option<Self> {
        let bytes = std::fs::read(path).ok()?;
        Self::from_bytes(&bytes)
    }

    /// Whether the decoded model can actually drive prediction.
    pub fn is_usable(&self) -> bool {
        !self.labels.is_empty() && !self.state_features.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The model shipped in the repository.
    fn shipped_model_path() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("data/crf_training/bunsetsu.crfsuite")
    }

    #[test]
    fn decodes_shipped_model() {
        let weights = CrfWeights::load(&shipped_model_path()).expect("shipped model must decode");

        assert!(weights.is_usable());
        assert_eq!(weights.labels.len(), 4, "expected the 4 bunsetsu labels");
        for label in ["B-L", "I-L", "B-P", "I-P"] {
            assert!(weights.labels.iter().any(|l| l == label), "missing label {label}");
        }
        assert!(!weights.state_features.is_empty());
        assert!(!weights.transitions.is_empty());
        // Every transition must reference known labels
        for (from, to) in weights.transitions.keys() {
            assert!(weights.labels.contains(from));
            assert!(weights.labels.contains(to));
        }
    }

    #[test]
    fn rejects_non_model_bytes() {
        assert!(CrfWeights::from_bytes(b"definitely not a model").is_none());
    }

    #[test]
    fn missing_file_yields_none() {
        assert!(CrfWeights::load(std::path::Path::new("/nonexistent/bunsetsu.crfsuite")).is_none());
    }
}
