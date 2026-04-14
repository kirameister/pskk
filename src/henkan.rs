use crate::util::{
    crf_compute_emission_scores, crf_nbest_viterbi, labels_to_bunsetsu, tokenize_line,
    BunsetsuSegment, CrfFeatureMaterials, Dictionary, NBestPath, StateFeatureWeights,
    TransitionWeights,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub surface: String,
    pub reading: String,
    pub count: i32,
    #[serde(default)]
    pub passthrough: bool,
    #[serde(default)]
    pub bunsetsu_mode: bool,
}

impl Candidate {
    pub fn new(surface: String, reading: String, count: i32) -> Self {
        Self {
            surface,
            reading,
            count,
            passthrough: false,
            bunsetsu_mode: false,
        }
    }

    pub fn passthrough(reading: String) -> Self {
        Self {
            surface: reading.clone(),
            reading,
            count: 0,
            passthrough: true,
            bunsetsu_mode: false,
        }
    }

    pub fn bunsetsu(surface: String, reading: String) -> Self {
        Self {
            surface,
            reading,
            count: 0,
            passthrough: false,
            bunsetsu_mode: true,
        }
    }
}

#[derive(Debug, Clone)]
struct BunsetsuPrediction {
    bunsetsu_list: Vec<BunsetsuSegment>,
    #[allow(dead_code)]
    score: f64,
}

pub struct HenkanProcessor {
    dictionary: Arc<Mutex<Dictionary>>,
    ready: Arc<Mutex<bool>>,
    
    candidates: Vec<Candidate>,
    selected_index: usize,
    
    current_yomi: String,
    has_whole_word_match: bool,
    
    bunsetsu_mode: bool,
    bunsetsu_predictions: Vec<BunsetsuPrediction>,
    bunsetsu_prediction_index: usize,
    bunsetsu_candidates: Vec<Vec<Candidate>>,
    bunsetsu_selected_indices: Vec<usize>,
    selected_bunsetsu_index: usize,
    
    crf_feature_materials: Option<CrfFeatureMaterials>,
    state_features: Option<StateFeatureWeights>,
    transitions: Option<TransitionWeights>,
    labels: Vec<String>,
}

impl HenkanProcessor {
    pub fn new() -> Self {
        Self {
            dictionary: Arc::new(Mutex::new(HashMap::new())),
            ready: Arc::new(Mutex::new(false)),
            candidates: Vec::new(),
            selected_index: 0,
            current_yomi: String::new(),
            has_whole_word_match: false,
            bunsetsu_mode: false,
            bunsetsu_predictions: Vec::new(),
            bunsetsu_prediction_index: 0,
            bunsetsu_candidates: Vec::new(),
            bunsetsu_selected_indices: Vec::new(),
            selected_bunsetsu_index: 0,
            crf_feature_materials: None,
            state_features: None,
            transitions: None,
            labels: vec![
                "B-L".to_string(),
                "I-L".to_string(),
                "B-P".to_string(),
                "I-P".to_string(),
            ],
        }
    }

    pub fn with_dictionary(self, dictionary: Dictionary) -> Self {
        *self.dictionary.lock().unwrap() = dictionary;
        *self.ready.lock().unwrap() = true;
        self
    }

    pub fn with_crf_model(
        mut self,
        feature_materials: CrfFeatureMaterials,
        state_features: StateFeatureWeights,
        transitions: TransitionWeights,
    ) -> Self {
        self.crf_feature_materials = Some(feature_materials);
        self.state_features = Some(state_features);
        self.transitions = Some(transitions);
        self
    }

    pub fn is_ready(&self) -> bool {
        *self.ready.lock().unwrap()
    }

    pub fn convert(&mut self, reading: &str) -> &[Candidate] {
        self.reset();
        self.current_yomi = reading.to_string();

        if !self.is_ready() {
            self.candidates.push(Candidate::passthrough(reading.to_string()));
            return &self.candidates;
        }

        let dict = self.dictionary.lock().unwrap();
        let has_match = dict.contains_key(reading);
        
        if has_match {
            self.has_whole_word_match = true;
            
            if let Some(candidates_map) = dict.get(reading) {
                let mut candidates: Vec<(String, i32)> = candidates_map
                    .iter()
                    .map(|(surface, &count)| (surface.clone(), count))
                    .collect();
                
                candidates.sort_by(|a, b| b.1.cmp(&a.1));
                
                for (surface, count) in candidates {
                    self.candidates.push(Candidate::new(
                        surface,
                        reading.to_string(),
                        count,
                    ));
                }
            }
        } else {
            self.has_whole_word_match = false;
            drop(dict);
            
            let predictions = self.predict_bunsetsu(reading, 5);
            self.bunsetsu_predictions = predictions
                .into_iter()
                .filter(|p| self.is_multi_bunsetsu(&p.bunsetsu_list))
                .collect();

            if !self.bunsetsu_predictions.is_empty() {
                self.init_bunsetsu_mode(0);
                
                let surface = self.get_display_surface();
                self.candidates.push(Candidate::bunsetsu(surface, reading.to_string()));
            } else {
                self.candidates.push(Candidate::passthrough(reading.to_string()));
            }
        }

        &self.candidates
    }

    pub fn get_candidates(&self) -> &[Candidate] {
        &self.candidates
    }

    pub fn select_candidate(&mut self, index: usize) -> Option<&Candidate> {
        if index < self.candidates.len() {
            self.selected_index = index;
            Some(&self.candidates[index])
        } else {
            None
        }
    }

    pub fn get_selected_candidate(&self) -> Option<&Candidate> {
        self.candidates.get(self.selected_index)
    }

    pub fn next_candidate(&mut self) -> Option<&Candidate> {
        if !self.candidates.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.candidates.len();
            Some(&self.candidates[self.selected_index])
        } else {
            None
        }
    }

    pub fn previous_candidate(&mut self) -> Option<&Candidate> {
        if !self.candidates.is_empty() {
            if self.selected_index == 0 {
                self.selected_index = self.candidates.len() - 1;
            } else {
                self.selected_index -= 1;
            }
            Some(&self.candidates[self.selected_index])
        } else {
            None
        }
    }

    pub fn reset(&mut self) {
        self.candidates.clear();
        self.selected_index = 0;
        self.bunsetsu_mode = false;
        self.current_yomi.clear();
        self.has_whole_word_match = false;
        self.bunsetsu_predictions.clear();
        self.bunsetsu_prediction_index = 0;
        self.bunsetsu_candidates.clear();
        self.bunsetsu_selected_indices.clear();
        self.selected_bunsetsu_index = 0;
    }

    fn predict_bunsetsu(&self, input_text: &str, n_best: usize) -> Vec<BunsetsuPrediction> {
        if input_text.is_empty() {
            return Vec::new();
        }

        let Some(ref feature_materials) = self.crf_feature_materials else {
            return Vec::new();
        };

        let Some(ref state_features) = self.state_features else {
            return Vec::new();
        };

        let Some(ref transitions) = self.transitions else {
            return Vec::new();
        };

        let tokens = tokenize_line(input_text);
        let features = crate::util::add_features_per_line(tokens.clone(), Some(feature_materials));
        
        let emission = crf_compute_emission_scores(&features, state_features, &self.labels);
        let nbest_results = crf_nbest_viterbi(&emission, transitions, &self.labels, n_best);

        nbest_results
            .into_iter()
            .map(|NBestPath { labels, score }| {
                let bunsetsu_list = labels_to_bunsetsu(&tokens, &labels);
                BunsetsuPrediction {
                    bunsetsu_list,
                    score,
                }
            })
            .collect()
    }

    fn is_multi_bunsetsu(&self, bunsetsu_list: &[BunsetsuSegment]) -> bool {
        bunsetsu_list.len() >= 2
    }

    fn init_bunsetsu_mode(&mut self, prediction_index: usize) {
        if self.bunsetsu_predictions.is_empty()
            || prediction_index >= self.bunsetsu_predictions.len()
        {
            return;
        }

        self.bunsetsu_prediction_index = prediction_index;
        self.bunsetsu_mode = true;

        let prediction = &self.bunsetsu_predictions[prediction_index];
        self.bunsetsu_candidates.clear();
        self.bunsetsu_selected_indices.clear();

        for segment in &prediction.bunsetsu_list {
            let candidates = if segment.label.starts_with("B-L") {
                self.lookup_bunsetsu_candidates(&segment.text)
            } else {
                vec![Candidate::passthrough(segment.text.clone())]
            };

            self.bunsetsu_selected_indices.push(0);
            self.bunsetsu_candidates.push(candidates);
        }

        self.selected_bunsetsu_index = 0;
    }

    fn lookup_bunsetsu_candidates(&self, bunsetsu_text: &str) -> Vec<Candidate> {
        let dict = self.dictionary.lock().unwrap();
        
        if let Some(candidates_map) = dict.get(bunsetsu_text) {
            let mut candidates: Vec<(String, i32)> = candidates_map
                .iter()
                .map(|(surface, &count)| (surface.clone(), count))
                .collect();
            
            candidates.sort_by(|a, b| b.1.cmp(&a.1));
            
            candidates
                .into_iter()
                .map(|(surface, count)| {
                    Candidate::new(surface, bunsetsu_text.to_string(), count)
                })
                .collect()
        } else {
            vec![Candidate::passthrough(bunsetsu_text.to_string())]
        }
    }

    pub fn is_bunsetsu_mode(&self) -> bool {
        self.bunsetsu_mode
    }

    pub fn get_bunsetsu_count(&self) -> usize {
        self.bunsetsu_candidates.len()
    }

    pub fn get_selected_bunsetsu_index(&self) -> usize {
        self.selected_bunsetsu_index
    }

    pub fn cycle_bunsetsu_prediction(&mut self) -> bool {
        if !self.has_whole_word_match && !self.bunsetsu_predictions.is_empty() {
            let next_index = (self.bunsetsu_prediction_index + 1) % self.bunsetsu_predictions.len();
            self.init_bunsetsu_mode(next_index);
            
            if !self.candidates.is_empty() {
                let surface = self.get_display_surface();
                self.candidates[0] = Candidate::bunsetsu(surface, self.current_yomi.clone());
            }
            
            true
        } else {
            false
        }
    }

    pub fn select_bunsetsu(&mut self, index: usize) -> bool {
        if index < self.bunsetsu_candidates.len() {
            self.selected_bunsetsu_index = index;
            true
        } else {
            false
        }
    }

    pub fn next_bunsetsu(&mut self) -> bool {
        if !self.bunsetsu_candidates.is_empty() {
            self.selected_bunsetsu_index =
                (self.selected_bunsetsu_index + 1) % self.bunsetsu_candidates.len();
            true
        } else {
            false
        }
    }

    pub fn previous_bunsetsu(&mut self) -> bool {
        if !self.bunsetsu_candidates.is_empty() {
            if self.selected_bunsetsu_index == 0 {
                self.selected_bunsetsu_index = self.bunsetsu_candidates.len() - 1;
            } else {
                self.selected_bunsetsu_index -= 1;
            }
            true
        } else {
            false
        }
    }

    pub fn next_bunsetsu_candidate(&mut self) -> bool {
        if self.selected_bunsetsu_index >= self.bunsetsu_candidates.len() {
            return false;
        }

        let candidates = &self.bunsetsu_candidates[self.selected_bunsetsu_index];
        if candidates.is_empty() {
            return false;
        }

        let current_idx = self.bunsetsu_selected_indices[self.selected_bunsetsu_index];
        let next_idx = (current_idx + 1) % candidates.len();
        self.bunsetsu_selected_indices[self.selected_bunsetsu_index] = next_idx;

        if !self.candidates.is_empty() {
            let surface = self.get_display_surface();
            self.candidates[0] = Candidate::bunsetsu(surface, self.current_yomi.clone());
        }

        true
    }

    pub fn previous_bunsetsu_candidate(&mut self) -> bool {
        if self.selected_bunsetsu_index >= self.bunsetsu_candidates.len() {
            return false;
        }

        let candidates = &self.bunsetsu_candidates[self.selected_bunsetsu_index];
        if candidates.is_empty() {
            return false;
        }

        let current_idx = self.bunsetsu_selected_indices[self.selected_bunsetsu_index];
        let next_idx = if current_idx == 0 {
            candidates.len() - 1
        } else {
            current_idx - 1
        };
        self.bunsetsu_selected_indices[self.selected_bunsetsu_index] = next_idx;

        if !self.candidates.is_empty() {
            let surface = self.get_display_surface();
            self.candidates[0] = Candidate::bunsetsu(surface, self.current_yomi.clone());
        }

        true
    }

    pub fn get_display_surface(&self) -> String {
        if !self.bunsetsu_mode || self.bunsetsu_candidates.is_empty() {
            return String::new();
        }

        let mut result = String::new();
        for (i, candidates) in self.bunsetsu_candidates.iter().enumerate() {
            if let Some(selected_idx) = self.bunsetsu_selected_indices.get(i) {
                if let Some(candidate) = candidates.get(*selected_idx) {
                    result.push_str(&candidate.surface);
                }
            }
        }
        result
    }

    pub fn get_display_surface_with_selection(&self) -> Vec<(String, bool)> {
        if !self.bunsetsu_mode || self.bunsetsu_candidates.is_empty() {
            return Vec::new();
        }

        let mut result = Vec::new();
        for (i, candidates) in self.bunsetsu_candidates.iter().enumerate() {
            if let Some(selected_idx) = self.bunsetsu_selected_indices.get(i) {
                if let Some(candidate) = candidates.get(*selected_idx) {
                    let is_selected = i == self.selected_bunsetsu_index;
                    result.push((candidate.surface.clone(), is_selected));
                }
            }
        }
        result
    }
}

impl Default for HenkanProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_dictionary() -> Dictionary {
        let mut dict = HashMap::new();
        
        let mut henkan_candidates = HashMap::new();
        henkan_candidates.insert("変換".to_string(), 100);
        henkan_candidates.insert("返還".to_string(), 50);
        henkan_candidates.insert("編纂".to_string(), 10);
        dict.insert("へんかん".to_string(), henkan_candidates);
        
        let mut kyou_candidates = HashMap::new();
        kyou_candidates.insert("今日".to_string(), 200);
        kyou_candidates.insert("京".to_string(), 80);
        kyou_candidates.insert("教".to_string(), 40);
        dict.insert("きょう".to_string(), kyou_candidates);
        
        let mut ha_candidates = HashMap::new();
        ha_candidates.insert("は".to_string(), 100);
        dict.insert("は".to_string(), ha_candidates);
        
        dict
    }

    #[test]
    fn not_ready_returns_passthrough() {
        let mut processor = HenkanProcessor::new();
        let candidates = processor.convert("へんかん");
        
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].surface, "へんかん");
        assert!(candidates[0].passthrough);
    }

    #[test]
    fn whole_word_match_returns_sorted_candidates() {
        let mut processor = HenkanProcessor::new().with_dictionary(sample_dictionary());
        let candidates = processor.convert("へんかん");
        
        assert_eq!(candidates.len(), 3);
        assert_eq!(candidates[0].surface, "変換");
        assert_eq!(candidates[0].count, 100);
        assert_eq!(candidates[1].surface, "返還");
        assert_eq!(candidates[1].count, 50);
        assert_eq!(candidates[2].surface, "編纂");
        assert_eq!(candidates[2].count, 10);
    }

    #[test]
    fn no_match_without_crf_returns_passthrough() {
        let mut processor = HenkanProcessor::new().with_dictionary(sample_dictionary());
        let candidates = processor.convert("みつからない");
        
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].surface, "みつからない");
        assert!(candidates[0].passthrough);
    }

    #[test]
    fn candidate_navigation() {
        let mut processor = HenkanProcessor::new().with_dictionary(sample_dictionary());
        processor.convert("へんかん");
        
        assert_eq!(processor.get_selected_candidate().unwrap().surface, "変換");
        
        processor.next_candidate();
        assert_eq!(processor.get_selected_candidate().unwrap().surface, "返還");
        
        processor.next_candidate();
        assert_eq!(processor.get_selected_candidate().unwrap().surface, "編纂");
        
        processor.next_candidate();
        assert_eq!(processor.get_selected_candidate().unwrap().surface, "変換");
        
        processor.previous_candidate();
        assert_eq!(processor.get_selected_candidate().unwrap().surface, "編纂");
    }

    #[test]
    fn reset_clears_state() {
        let mut processor = HenkanProcessor::new().with_dictionary(sample_dictionary());
        processor.convert("へんかん");
        processor.next_candidate();
        
        assert!(!processor.get_candidates().is_empty());
        
        processor.reset();
        
        assert!(processor.get_candidates().is_empty());
        assert_eq!(processor.selected_index, 0);
        assert!(!processor.is_bunsetsu_mode());
    }
}
