//! Opt-in bounded response-only diagnostics for the private A04 Plan fixture.

use serde_json::{Value, json};

const MAX_CAPTURE_JSON_BYTES: usize = 2 * 1024;
// `false` is the larger flag spelling; retain that allowance even after truncation.
const CAPTURE_OVERHEAD_BYTES: usize = br#"{"document_utf8":"","truncated":false}"#.len();

#[derive(Debug, Clone, Copy, Default)]
pub(super) enum PlanReplyCaptureMode {
    #[default]
    Off,
    A04Plan,
}

impl PlanReplyCaptureMode {
    pub(super) fn start(self, phase: &str) -> Option<BoundedPlanReplyCapture> {
        (matches!(self, Self::A04Plan) && phase == "plan").then(BoundedPlanReplyCapture::default)
    }
}

#[derive(Debug, Default)]
pub(super) struct BoundedPlanReplyCapture {
    document: String,
    encoded_text_bytes: usize,
    truncated: bool,
}

impl BoundedPlanReplyCapture {
    pub(super) fn push(&mut self, text: &str) {
        if self.truncated {
            return;
        }
        for character in text.chars() {
            let bytes = match character {
                '"' | '\\' | '\u{08}' | '\u{0c}' | '\n' | '\r' | '\t' => 2,
                '\u{00}'..='\u{1f}' => 6,
                _ => character.len_utf8(),
            };
            if bytes > MAX_CAPTURE_JSON_BYTES - CAPTURE_OVERHEAD_BYTES - self.encoded_text_bytes {
                self.truncated = true;
                break;
            }
            self.document.push(character);
            self.encoded_text_bytes += bytes;
        }
    }

    pub(super) fn value(&self) -> Value {
        json!({"document_utf8":self.document,"truncated":self.truncated})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    type TestResult = Result<(), Box<dyn Error>>;

    #[test]
    fn capture_defaults_off_and_explicit_a04_selection_still_excludes_edit() {
        assert!(PlanReplyCaptureMode::default().start("plan").is_none());
        assert!(PlanReplyCaptureMode::A04Plan.start("edit").is_none());
        assert!(PlanReplyCaptureMode::A04Plan.start("intake").is_none());
        assert!(PlanReplyCaptureMode::A04Plan.start("plan").is_some());
    }

    #[test]
    fn plan_document_is_preserved_exactly_across_unicode_chunks() -> TestResult {
        let text = r#"{"version":1,"steps":[{"offer":"P1","outcome":"CLI prüfen 東京 🚀"}]}"#;
        let mut capture = BoundedPlanReplyCapture::default();
        for character in text.chars() {
            let mut utf8 = [0; 4];
            capture.push(character.encode_utf8(&mut utf8));
        }
        assert_eq!(capture.value()["document_utf8"], text);
        assert_eq!(capture.value()["truncated"], false);
        assert!(serde_json::to_vec(&capture.value())?.len() <= MAX_CAPTURE_JSON_BYTES);
        Ok(())
    }

    #[test]
    fn exact_serialized_limit_and_oversize_never_exceed_two_kib() -> TestResult {
        let mut capture = BoundedPlanReplyCapture::default();
        let exact = "a".repeat(MAX_CAPTURE_JSON_BYTES - CAPTURE_OVERHEAD_BYTES);
        capture.push(&exact);
        assert_eq!(
            serde_json::to_vec(&capture.value())?.len(),
            MAX_CAPTURE_JSON_BYTES
        );
        assert!(!capture.truncated);
        capture.push("x");
        assert!(capture.truncated);
        assert_eq!(capture.document, exact);
        assert!(serde_json::to_vec(&capture.value())?.len() <= MAX_CAPTURE_JSON_BYTES);
        Ok(())
    }

    #[test]
    fn escaping_and_unicode_charge_actual_json_bytes_and_keep_a_true_prefix() -> TestResult {
        for text in ["\0\"\\\n".repeat(2_048), "ä東京🚀".repeat(2_048)] {
            let mut capture = BoundedPlanReplyCapture::default();
            capture.push(&text);
            assert!(capture.truncated);
            assert!(text.starts_with(&capture.document));
            assert!(serde_json::to_vec(&capture.value())?.len() <= MAX_CAPTURE_JSON_BYTES);
            let first = capture.document.clone();
            capture.push("later short output");
            assert_eq!(capture.document, first);
        }
        Ok(())
    }
}
