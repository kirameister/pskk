use std::collections::{HashMap, HashSet};

/// Placeholder character used when a defined key pair has no assigned kanji.
pub const MISSING_KANCHOKU_KANJI: &str = "無";

/// Nested mapping of first stroke -> second stroke -> output kanji.
/// This is equivalent to dict(dict())
pub type KanchokuLayout = HashMap<char, HashMap<char, String>>;

/// Return value for [`KanchokuProcessor::process_key`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessResult {
    pub output: Option<String>,
    pub pending: Option<char>,
    pub consumed: bool,
}
//// This is like the implementation section of defined/declared return-value types??
impl ProcessResult {
    fn new(output: Option<String>, pending: Option<char>, consumed: bool) -> Self {
        Self {
            output,
            pending,
            consumed,
        }
    }
}

/// Processor for two-stroke kanchoku direct kanji input.
#[derive(Debug, Clone, Default)]
pub struct KanchokuProcessor {
    layout: KanchokuLayout,
    first_stroke: Option<char>,
}

impl KanchokuProcessor {
    /// Create a processor from an optional layout.
    pub fn new(layout: Option<KanchokuLayout>) -> Self {
        let mut processor = Self {
            layout: layout.unwrap_or_default(),
            first_stroke: None,
        };
        processor.reset();
        processor
    }

    /// Reset internal state and clear any pending first stroke.
    pub fn reset(&mut self) {
        self.first_stroke = None;
    }

    /// Whether the processor is waiting for the second stroke.
    pub fn is_waiting_for_second_stroke(&self) -> bool {
        self.first_stroke.is_some()
    }

    /// Return the currently pending first stroke, if any.
    pub fn first_stroke(&self) -> Option<char> {
        self.first_stroke
    }

    /// Process one key event.
    pub fn process_key(&mut self, key_char: char, is_pressed: bool) -> ProcessResult {
        if !is_pressed {
            return ProcessResult::new(None, self.first_stroke, false);
        }

        if self.first_stroke.is_none() {
            if !self.layout.contains_key(&key_char) {
                return ProcessResult::new(None, None, false);
            }

            self.first_stroke = Some(key_char);
            return ProcessResult::new(None, Some(key_char), true);
        }

        let first = self.first_stroke.expect("checked above");
        let second = key_char;

        let valid_second = self
            .layout
            .get(&first)
            .is_some_and(|row| row.contains_key(&second));

        if !valid_second {
            self.reset();
            return ProcessResult::new(Some(first.to_string()), None, false);
        }

        let kanji = self.lookup_kanji(first, second);
        self.reset();
        ProcessResult::new(Some(kanji), None, true)
    }

    /// Look up the kanji assigned to a two-stroke pair.
    pub fn lookup_kanji(&self, first_key: char, second_key: char) -> String {
        let Some(row) = self.layout.get(&first_key) else {
            return MISSING_KANCHOKU_KANJI.to_string();
        };

        match row.get(&second_key) {
            Some(kanji) if !kanji.is_empty() => kanji.clone(),
            _ => MISSING_KANCHOKU_KANJI.to_string(),
        }
    }

    /// Cancel the current sequence and return any pending first stroke.
    pub fn cancel(&mut self) -> Option<char> {
        let first = self.first_stroke;
        self.reset();
        first
    }

    /// Return the set of valid first-stroke keys.
    pub fn valid_keys(&self) -> HashSet<char> {
        self.layout.keys().copied().collect()
    }

    /// Return valid second-stroke keys for the provided first stroke.
    pub fn second_stroke_keys(&self, first_stroke: char) -> HashSet<char> {
        self.layout
            .get(&first_stroke)
            .map(|row| row.keys().copied().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::{KanchokuLayout, KanchokuProcessor, MISSING_KANCHOKU_KANJI};
    use std::collections::{HashMap, HashSet};

    fn sample_layout() -> KanchokuLayout {
        HashMap::from([
            (
                'j',
                HashMap::from([
                    ('k', "日".to_string()),
                    ('l', "月".to_string()),
                    ('x', String::new()),
                ]),
            ),
            ('a', HashMap::from([('s', "本".to_string())])),
        ])
    }

    #[test]
    fn ignores_key_release() {
        let mut processor = KanchokuProcessor::new(Some(sample_layout()));
        let result = processor.process_key('j', false);

        assert_eq!(result.output, None);
        assert_eq!(result.pending, None);
        assert!(!result.consumed);
    }

    #[test]
    fn first_stroke_enters_waiting_state() {
        let mut processor = KanchokuProcessor::new(Some(sample_layout()));
        let result = processor.process_key('j', true);

        assert_eq!(result.output, None);
        assert_eq!(result.pending, Some('j'));
        assert!(result.consumed);
        assert!(processor.is_waiting_for_second_stroke());
    }

    #[test]
    fn valid_second_stroke_outputs_kanji() {
        let mut processor = KanchokuProcessor::new(Some(sample_layout()));
        processor.process_key('j', true);
        let result = processor.process_key('k', true);

        assert_eq!(result.output, Some("日".to_string()));
        assert_eq!(result.pending, None);
        assert!(result.consumed);
        assert!(!processor.is_waiting_for_second_stroke());
    }

    #[test]
    fn invalid_second_stroke_returns_first_key_and_resets() {
        let mut processor = KanchokuProcessor::new(Some(sample_layout()));
        processor.process_key('j', true);
        let result = processor.process_key('q', true);

        assert_eq!(result.output, Some("j".to_string()));
        assert_eq!(result.pending, None);
        assert!(!result.consumed);
        assert_eq!(processor.first_stroke(), None);
    }

    #[test]
    fn missing_output_uses_placeholder() {
        let processor = KanchokuProcessor::new(Some(sample_layout()));
        assert_eq!(processor.lookup_kanji('j', 'x'), MISSING_KANCHOKU_KANJI);
        assert_eq!(processor.lookup_kanji('z', 'x'), MISSING_KANCHOKU_KANJI);
    }

    #[test]
    fn cancel_returns_pending_stroke() {
        let mut processor = KanchokuProcessor::new(Some(sample_layout()));
        processor.process_key('a', true);

        assert_eq!(processor.cancel(), Some('a'));
        assert_eq!(processor.first_stroke(), None);
    }

    #[test]
    fn returns_valid_key_sets() {
        let processor = KanchokuProcessor::new(Some(sample_layout()));

        assert_eq!(processor.valid_keys(), HashSet::from(['j', 'a']));
        assert_eq!(processor.second_stroke_keys('j'), HashSet::from(['k', 'l', 'x']));
        assert!(processor.second_stroke_keys('z').is_empty());
    }
}
