//! Touch mode or mouse mode (#331): on a phone with a mouse attached the game follows the last
//! input used. A finger puts it in touch mode (the touch top bar and gestures, no cursor); a
//! mouse movement, button or wheel puts it in mouse mode (the classic top bar and the game's
//! own cursor). Pure: the window reports what arrived, the game applies the mode.

use crate::ui::settings::TopBar;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputMode {
    Touch,
    Mouse,
}

impl InputMode {
    /// The mode at start: touch on Android, mouse elsewhere (where it never changes).
    pub fn initial() -> Self {
        if cfg!(target_os = "android") { Self::Touch } else { Self::Mouse }
    }

    /// The mode after an input of `kind`; `self` where the platform has one mode only.
    pub fn after(self, kind: Self) -> Self {
        if cfg!(target_os = "android") { kind } else { self }
    }

    /// The top bar shown: the `top_bar` setting when the player set it, else the mode's own.
    pub fn top_bar(self, setting: Option<TopBar>) -> TopBar {
        setting.unwrap_or(match self {
            Self::Touch if cfg!(target_os = "android") => TopBar::Touch,
            _ => TopBar::Classic,
        })
    }

    /// The game draws its own cursor: a mouse is the pointer (a finger has none).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn draws_cursor(self) -> bool {
        self == Self::Mouse
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_the_last_input_on_android_only() {
        let after_mouse = InputMode::Touch.after(InputMode::Mouse);
        let after_touch = InputMode::Mouse.after(InputMode::Touch);
        if cfg!(target_os = "android") {
            assert_eq!((after_mouse, after_touch), (InputMode::Mouse, InputMode::Touch));
        } else {
            assert_eq!((after_mouse, after_touch), (InputMode::Touch, InputMode::Mouse));
            assert_eq!(InputMode::initial(), InputMode::Mouse);
        }
    }

    #[test]
    fn an_explicit_top_bar_wins_in_both_modes() {
        for mode in [InputMode::Touch, InputMode::Mouse] {
            assert_eq!(mode.top_bar(Some(TopBar::Touch)), TopBar::Touch);
            assert_eq!(mode.top_bar(Some(TopBar::Classic)), TopBar::Classic);
        }
    }

    #[test]
    fn an_unset_top_bar_follows_the_mode() {
        assert_eq!(InputMode::Mouse.top_bar(None), TopBar::Classic);
        let touch = if cfg!(target_os = "android") { TopBar::Touch } else { TopBar::Classic };
        assert_eq!(InputMode::Touch.top_bar(None), touch);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn only_a_mouse_draws_the_cursor() {
        assert!(InputMode::Mouse.draws_cursor());
        assert!(!InputMode::Touch.draws_cursor());
    }
}
