//! Lazy system-font fallback (D-07, UI-05).
//!
//! eframe's built-in fallback is disabled (`system_font_fallback: false`) because it
//! permanently caches "no glyph" for characters it never saw. Instead we install egui's
//! system font provider only when a displayed character actually has no glyph in the
//! bundled fonts. ASCII-only files never start the provider's font-enumeration thread,
//! so startup and idle footprint stay at Phase 1 levels.

pub const MISSING_GLYPHS_NOTICE: &str = "Some characters can't be shown: no installed font has them";

use eframe::egui;

#[derive(Debug, PartialEq, Eq)]
pub enum FontAction {
    Nothing,
    Install,
    NoticeMissing,
}

#[derive(Debug, PartialEq, Eq)]
enum State {
    NotInstalled,
    Installed { missing_checks: u8 },
    Done,
}

pub struct FontFallback {
    state: State,
}

impl Default for FontFallback {
    fn default() -> Self {
        Self::new()
    }
}

impl FontFallback {
    pub fn new() -> Self {
        Self {
            state: State::NotInstalled,
        }
    }

    /// Pure state machine, unit-tested with a mock glyph predicate.
    pub fn decide(
        &mut self,
        text: &str,
        mut has_glyph: impl FnMut(char) -> bool,
    ) -> FontAction {
        if self.state == State::Done || text.is_ascii() {
            return FontAction::Nothing;
        }

        match self.state {
            State::NotInstalled => {
                if text.chars().any(|c| !c.is_ascii() && !has_glyph(c)) {
                    self.state = State::Installed { missing_checks: 0 };
                    FontAction::Install
                } else {
                    FontAction::Nothing
                }
            }
            State::Installed { missing_checks } => {
                if text.chars().any(|c| !c.is_ascii() && !has_glyph(c)) {
                    let n = missing_checks + 1;
                    if n >= 3 {
                        self.state = State::Done;
                        FontAction::NoticeMissing
                    } else {
                        self.state = State::Installed { missing_checks: n };
                        FontAction::Nothing
                    }
                } else {
                    self.state = State::Done;
                    FontAction::Nothing
                }
            }
            State::Done => FontAction::Nothing,
        }
    }

    /// Call when a newly decoded visible paragraph is laid out.
    ///
    /// Fast path: ASCII text returns `None` without touching the font stack. On install,
    /// the system provider is added once and one repaint is requested. Returns
    /// [`MISSING_GLYPHS_NOTICE`] exactly once per session.
    pub fn check(
        &mut self,
        ctx: &egui::Context,
        font_id: &egui::FontId,
        text: &str,
    ) -> Option<&'static str> {
        if text.is_ascii() || self.state == State::Done {
            return None;
        }
        let action = self.decide(text, |c| {
            ctx.fonts_mut(|f| f.has_glyph(font_id, c))
        });
        match action {
            FontAction::Nothing => None,
            FontAction::Install => {
                ctx.add_font_provider(std::sync::Arc::new(
                    egui_system_font_provider::SystemFontProvider::new(),
                ));
                ctx.request_repaint();
                None
            }
            FontAction::NoticeMissing => Some(MISSING_GLYPHS_NOTICE),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn missing(_: char) -> bool {
        false
    }

    fn present(_: char) -> bool {
        true
    }

    #[test]
    fn ascii_never_checks() {
        let mut fb = FontFallback::new();
        let mut calls = 0;
        let action = fb.decide("plain ascii", |_| {
            calls += 1;
            false
        });
        assert_eq!(action, FontAction::Nothing);
        assert_eq!(calls, 0);
    }

    #[test]
    fn first_missing_glyph_installs_once() {
        let mut fb = FontFallback::new();
        assert_eq!(fb.decide("a é", missing), FontAction::Install);
        assert_eq!(fb.decide("a 日", missing), FontAction::Nothing);
    }

    #[test]
    fn third_missing_check_notices_once_then_stops() {
        let mut fb = FontFallback::new();
        assert_eq!(fb.decide("日", missing), FontAction::Install);
        assert_eq!(fb.decide("日", missing), FontAction::Nothing);
        assert_eq!(fb.decide("日", missing), FontAction::Nothing);
        assert_eq!(fb.decide("日", missing), FontAction::NoticeMissing);
        assert_eq!(fb.decide("日", missing), FontAction::Nothing);
        assert_eq!(fb.decide("日", missing), FontAction::Nothing);
    }

    #[test]
    fn install_then_glyphs_present_is_done() {
        let mut fb = FontFallback::new();
        assert_eq!(fb.decide("日", missing), FontAction::Install);
        assert_eq!(fb.decide("日", present), FontAction::Nothing);
        // Done: later frames never consult the font stack again.
        let mut calls = 0;
        assert_eq!(
            fb.decide("日本語", |_| {
                calls += 1;
                false
            }),
            FontAction::Nothing
        );
        assert_eq!(calls, 0);
    }

    #[test]
    fn existing_glyphs_do_not_install() {
        let mut fb = FontFallback::new();
        assert_eq!(fb.decide("café", present), FontAction::Nothing);
        assert_eq!(fb.decide("日本語", present), FontAction::Nothing);
    }
}
