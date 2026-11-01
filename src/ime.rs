//! IME composition probe: in-memory only, the mapped file is never written.

use eframe::egui::ImeEvent;

/// Cap on the committed probe text (T-01-16).
pub const MAX_PROBE_CHARS: usize = 1024;

#[derive(Debug, Default)]
pub struct ImeProbe {
    preedit: String,
    committed: String,
    composing: bool,
}

impl ImeProbe {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply(&mut self, ev: &ImeEvent) {
        match ev {
            ImeEvent::Preedit { text, .. } => {
                // Empty preedit = composition ended (incl. Esc-cancel on Windows).
                self.composing = !text.is_empty();
                self.preedit = text.clone();
            }
            ImeEvent::Commit(text) => {
                self.preedit.clear();
                self.composing = false;
                self.committed.push_str(text);
                let count = self.committed.chars().count();
                if count > MAX_PROBE_CHARS {
                    self.committed = self.committed.chars().skip(count - MAX_PROBE_CHARS).collect();
                }
            }
            _ => {}
        }
    }

    pub fn escape(&mut self) {
        if !self.composing {
            self.committed.clear();
        }
    }

    pub fn preedit(&self) -> &str {
        &self.preedit
    }

    pub fn committed(&self) -> &str {
        &self.committed
    }

    pub fn is_composing(&self) -> bool {
        self.composing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pre(s: &str) -> ImeEvent {
        ImeEvent::Preedit { text: s.to_owned(), active_range_chars: None }
    }

    #[test]
    fn new_is_empty() {
        let p = ImeProbe::new();
        assert_eq!(p.preedit(), "");
        assert_eq!(p.committed(), "");
        assert!(!p.is_composing());
    }

    #[test]
    fn preedit_starts_composition() {
        let mut p = ImeProbe::new();
        p.apply(&pre("にほんご"));
        assert!(p.is_composing());
        assert_eq!(p.preedit(), "にほんご");
    }

    #[test]
    fn empty_preedit_ends_composition() {
        let mut p = ImeProbe::new();
        p.apply(&pre("に"));
        p.apply(&pre(""));
        assert!(!p.is_composing());
        assert_eq!(p.preedit(), "");
    }

    #[test]
    fn commit_appends_and_clears_preedit() {
        let mut p = ImeProbe::new();
        p.apply(&pre("にほんご"));
        p.apply(&ImeEvent::Commit("日本語".into()));
        assert_eq!(p.committed(), "日本語");
        assert_eq!(p.preedit(), "");
        assert!(!p.is_composing());
    }

    #[test]
    fn commits_accumulate() {
        let mut p = ImeProbe::new();
        p.apply(&ImeEvent::Commit("日本".into()));
        p.apply(&ImeEvent::Commit("語".into()));
        assert_eq!(p.committed(), "日本語");
    }

    #[test]
    fn escape_semantics() {
        let mut p = ImeProbe::new();
        p.apply(&ImeEvent::Commit("日本語".into()));
        p.apply(&pre("に"));
        p.escape();
        assert_eq!(p.committed(), "日本語");
        p.apply(&pre(""));
        p.escape();
        assert_eq!(p.committed(), "");
    }

    #[test]
    fn committed_is_capped() {
        let mut p = ImeProbe::new();
        p.apply(&ImeEvent::Commit("a".repeat(1000)));
        p.apply(&ImeEvent::Commit("b".repeat(100)));
        assert_eq!(p.committed().chars().count(), MAX_PROBE_CHARS);
        assert!(p.committed().ends_with('b'));
        assert!(p.committed().starts_with('a'));
    }

    #[test]
    fn delete_surrounding_ignored() {
        let mut p = ImeProbe::new();
        p.apply(&ImeEvent::Commit("x".into()));
        p.apply(&ImeEvent::DeleteSurrounding { before_chars: 1, after_chars: 0 });
        assert_eq!(p.committed(), "x");
    }
}
