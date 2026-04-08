use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutEntry {
    pub output: String,
    pub pending: String,
    pub simul_limit_ms: Option<u64>,
}

impl LayoutEntry {
    pub fn new(output: impl Into<String>, pending: impl Into<String>, simul_limit_ms: Option<u64>) -> Self {
        Self {
            output: output.into(),
            pending: pending.into(),
            simul_limit_ms,
        }
    }
}

pub type RawLayoutEntry = (String, String, String, Option<u64>);

#[derive(Debug, Clone, Default)]
pub struct SimultaneousInputProcessor {
    pub layout_data: Option<Vec<RawLayoutEntry>>,
    pub simultaneous_map: Vec<HashMap<String, LayoutEntry>>,
    pub max_simul_limit_ms: u64,
    pub previous_typed_timestamp_ms: f64,
    clock_origin: Option<Instant>,
}

impl SimultaneousInputProcessor {
    pub fn new(layout_data: Option<Vec<RawLayoutEntry>>) -> Self {
        let clock_origin = Instant::now();
        let mut processor = Self {
            layout_data,
            simultaneous_map: Vec::new(),
            max_simul_limit_ms: 0,
            previous_typed_timestamp_ms: 0.0,
            clock_origin: Some(clock_origin),
        };
        processor.build_simultaneous_map();
        processor.previous_typed_timestamp_ms = -(processor.max_simul_limit_ms as f64 * 1000.0);
        processor
    }

    pub fn build_simultaneous_map(&mut self) {
        let Some(layout_data) = self.layout_data.as_ref() else {
            return;
        };
        if layout_data.is_empty() {
            return;
        }

        let max_input_len = layout_data
            .iter()
            .map(|entry| entry.0.chars().count())
            .max()
            .unwrap_or(0);

        if max_input_len == 0 {
            self.simultaneous_map.clear();
            return;
        }

        self.simultaneous_map = vec![HashMap::new(); max_input_len];

        for (input_str, output, pending, simul_limit_ms) in layout_data {
            let input_len = input_str.chars().count();
            if input_len == 0 {
                continue;
            }

            if let Some(limit) = simul_limit_ms {
                self.max_simul_limit_ms = self.max_simul_limit_ms.max(*limit);
            }

            self.simultaneous_map[input_len - 1].insert(
                input_str.clone(),
                LayoutEntry::new(output.clone(), pending.clone(), *simul_limit_ms),
            );
        }
    }

    pub fn simultaneous_reset(&mut self) {
        self.previous_typed_timestamp_ms -= self.max_simul_limit_ms as f64 * 1000.0;
    }

    pub fn get_layout_output(
        &mut self,
        past_pending: &str,
        input_char: &str,
        is_pressed: bool,
    ) -> (Option<String>, Option<String>) {
        let current_time_ms = self.current_time_ms();
        self.get_layout_output_at(past_pending, input_char, is_pressed, current_time_ms)
    }

    pub fn get_layout_output_at(
        &mut self,
        past_pending: &str,
        input_char: &str,
        is_pressed: bool,
        current_time_ms: f64,
    ) -> (Option<String>, Option<String>) {
        if !is_pressed {
            self.simultaneous_reset();
            return (None, None);
        }

        let time_diff_ms = current_time_ms - self.previous_typed_timestamp_ms;
        let pending_chars: Vec<char> = past_pending.chars().collect();

        if self.simultaneous_map.is_empty() {
            self.previous_typed_timestamp_ms = current_time_ms;
            return (Some(format!("{past_pending}{input_char}")), None);
        }

        let max_tail_len = pending_chars.len().min(self.simultaneous_map.len().saturating_sub(1));

        for tail_len in (0..=max_tail_len).rev() {
            let pending_tail: String = if tail_len > 0 {
                pending_chars[pending_chars.len() - tail_len..].iter().collect()
            } else {
                String::new()
            };
            let lookup_key = format!("{pending_tail}{input_char}");
            let dropped_prefix: String = if tail_len > 0 {
                pending_chars[..pending_chars.len() - tail_len].iter().collect()
            } else {
                past_pending.to_string()
            };

            let key_idx = lookup_key.chars().count() - 1;
            let Some(entry) = self
                .simultaneous_map
                .get(key_idx)
                .and_then(|bucket| bucket.get(&lookup_key))
            else {
                continue;
            };

            if let Some(simul_limit) = entry.simul_limit_ms {
                if simul_limit > 0 && time_diff_ms >= simul_limit as f64 {
                    continue;
                }
            }

            self.previous_typed_timestamp_ms = current_time_ms;
            return (
                Some(format!("{dropped_prefix}{}", entry.output)),
                Some(entry.pending.clone()),
            );
        }

        self.previous_typed_timestamp_ms = current_time_ms;
        (Some(format!("{past_pending}{input_char}")), None)
    }

    fn current_time_ms(&self) -> f64 {
        self.clock_origin
            .unwrap_or_else(Instant::now)
            .elapsed()
            .as_secs_f64()
            * 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::{RawLayoutEntry, SimultaneousInputProcessor};

    fn entry(input: &str, output: &str, pending: &str, simul_limit_ms: Option<u64>) -> RawLayoutEntry {
        (
            input.to_string(),
            output.to_string(),
            pending.to_string(),
            simul_limit_ms,
        )
    }

    #[test]
    fn empty_layout_is_handled() {
        let processor = SimultaneousInputProcessor::new(Some(vec![]));
        assert!(processor.simultaneous_map.is_empty());
    }

    #[test]
    fn none_layout_is_handled() {
        let processor = SimultaneousInputProcessor::new(None);
        assert!(processor.simultaneous_map.is_empty());
    }

    #[test]
    fn builds_map_and_tracks_max_simul_limit() {
        let processor = SimultaneousInputProcessor::new(Some(vec![
            entry("a", "あ", "", None),
            entry("jk", "じゅ", "", Some(50)),
            entry("kya", "きゃ", "", None),
            entry("df", "だ", "", Some(80)),
        ]));

        assert_eq!(processor.simultaneous_map.len(), 3);
        assert_eq!(
            processor.simultaneous_map[0]["a"].simul_limit_ms,
            None
        );
        assert_eq!(
            processor.simultaneous_map[1]["jk"].simul_limit_ms,
            Some(50)
        );
        assert_eq!(processor.max_simul_limit_ms, 80);
    }

    #[test]
    fn skips_empty_input_entries() {
        let processor = SimultaneousInputProcessor::new(Some(vec![
            entry("", "empty", "", None),
            entry("a", "あ", "", None),
        ]));

        assert_eq!(processor.simultaneous_map.len(), 1);
        assert!(processor.simultaneous_map[0].contains_key("a"));
        assert!(!processor.simultaneous_map[0].contains_key(""));
    }

    #[test]
    fn release_resets_and_returns_none_tuple() {
        let mut processor =
            SimultaneousInputProcessor::new(Some(vec![entry("jk", "じゅ", "", Some(50))]));
        processor.previous_typed_timestamp_ms = 1000.0;

        let result = processor.get_layout_output_at("abc", "x", false, 1001.0);

        assert_eq!(result, (None, None));
        assert_eq!(processor.previous_typed_timestamp_ms, 1000.0 - 50_000.0);
    }

    #[test]
    fn single_char_match() {
        let mut processor = SimultaneousInputProcessor::new(Some(vec![entry("a", "あ", "", None)]));
        let (output, pending) = processor.get_layout_output_at("", "a", true, 1.0);

        assert_eq!(output, Some("あ".to_string()));
        assert_eq!(pending, Some(String::new()));
    }

    #[test]
    fn multi_char_match() {
        let mut processor = SimultaneousInputProcessor::new(Some(vec![
            entry("k", "", "k", None),
            entry("ka", "か", "", None),
        ]));
        let (output, pending) = processor.get_layout_output_at("k", "a", true, 1.0);

        assert_eq!(output, Some("か".to_string()));
        assert_eq!(pending, Some(String::new()));
    }

    #[test]
    fn pending_char_waits_for_follow_up() {
        let mut processor = SimultaneousInputProcessor::new(Some(vec![entry("k", "", "k", None)]));
        let (output, pending) = processor.get_layout_output_at("", "k", true, 1.0);

        assert_eq!(output, Some(String::new()));
        assert_eq!(pending, Some("k".to_string()));
    }

    #[test]
    fn no_match_returns_input_as_output() {
        let mut processor = SimultaneousInputProcessor::new(Some(vec![entry("a", "あ", "", None)]));
        let (output, pending) = processor.get_layout_output_at("", "x", true, 1.0);

        assert_eq!(output, Some("x".to_string()));
        assert_eq!(pending, None);
    }

    #[test]
    fn simultaneous_within_time_window_matches() {
        let mut processor = SimultaneousInputProcessor::new(Some(vec![
            entry("a", "あ", "", None),
            entry("k", "", "k", None),
            entry("jk", "じゅ", "", Some(50)),
        ]));
        processor.previous_typed_timestamp_ms = 0.0;

        let (output, pending) = processor.get_layout_output_at("j", "k", true, 30.0);

        assert_eq!(output, Some("じゅ".to_string()));
        assert_eq!(pending, Some(String::new()));
    }

    #[test]
    fn simultaneous_timeout_falls_back_to_shorter_key() {
        let mut processor = SimultaneousInputProcessor::new(Some(vec![
            entry("k", "", "k", None),
            entry("jk", "じゅ", "", Some(50)),
        ]));
        processor.previous_typed_timestamp_ms = 0.0;

        let (output, pending) = processor.get_layout_output_at("j", "k", true, 100.0);

        assert_eq!(output, Some("j".to_string()));
        assert_eq!(pending, Some("k".to_string()));
    }

    #[test]
    fn dropped_prefix_is_included() {
        let mut processor = SimultaneousInputProcessor::new(Some(vec![
            entry("a", "あ", "", None),
            entry("c", "っ", "", None),
        ]));

        let (output, pending) = processor.get_layout_output_at("ab", "c", true, 1.0);

        assert_eq!(output, Some("abっ".to_string()));
        assert_eq!(pending, Some(String::new()));
    }

    #[test]
    fn long_pending_uses_supported_key_lengths() {
        let mut processor = SimultaneousInputProcessor::new(Some(vec![
            entry("a", "あ", "", None),
            entry("xa", "特", "", None),
        ]));

        let (output, pending) = processor.get_layout_output_at("xyz", "a", true, 1.0);

        assert_eq!(output, Some("xyzあ".to_string()));
        assert_eq!(pending, Some(String::new()));
    }

    #[test]
    fn fallback_chain_uses_shorter_match() {
        let mut processor = SimultaneousInputProcessor::new(Some(vec![
            entry("c", "C", "", None),
            entry("bc", "BC", "", None),
        ]));

        let (output, pending) = processor.get_layout_output_at("ab", "c", true, 1.0);

        assert_eq!(output, Some("aBC".to_string()));
        assert_eq!(pending, Some(String::new()));
    }

    #[test]
    fn pending_value_is_preserved() {
        let mut processor = SimultaneousInputProcessor::new(Some(vec![
            entry("k", "", "k", None),
            entry("ka", "か", "", None),
            entry("ky", "", "ky", None),
        ]));

        let (output, pending) = processor.get_layout_output_at("", "k", true, 1.0);
        assert_eq!(output, Some(String::new()));
        assert_eq!(pending, Some("k".to_string()));

        let (output, pending) = processor.get_layout_output_at("k", "y", true, 2.0);
        assert_eq!(output, Some(String::new()));
        assert_eq!(pending, Some("ky".to_string()));
    }
}
