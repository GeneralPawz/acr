//! The app going to the background and coming back (Android's app switch, winit's
//! `suspended`/`resumed`): what is paused, and the autosave slot written before the system
//! can kill the process.

use crate::ui::screens::save_load::file_name;

/// The savegame written when the app is suspended in a running game (CP1252, like a typed
/// name); the Load menu lists it like any other save, newest first.
pub const AUTOSAVE: &[u8] = b"Autosave (App)";

/// The store name of the autosave (`Autosave (App).sav`).
pub fn autosave_file() -> String {
    file_name(AUTOSAVE)
}

/// Whether the suspend paused the game. A resume never unpauses: the player resumes with
/// the pause plaque, the play gesture or the Pause key, and a game the user paused stays
/// the user's.
#[derive(Default, Debug)]
pub struct Suspension {
    paused_game: bool,
}

impl Suspension {
    /// The app is suspended while the game is (`game_paused`) or is not paused. True if
    /// the game is to be paused now. A second suspend without a resume keeps the first one's
    /// note.
    pub fn suspend(&mut self, game_paused: bool) -> bool {
        let pause = !game_paused;
        self.paused_game |= pause;
        pause
    }

    /// The app is back: true if the suspend had paused the game (it stays paused).
    pub fn resume(&mut self) -> bool {
        std::mem::take(&mut self.paused_game)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_running_game_is_paused_by_the_suspend_and_stays_paused() {
        let mut s = Suspension::default();
        assert!(s.suspend(false), "a running game is paused");
        assert!(s.resume(), "the suspend had paused it");
        assert!(!s.resume(), "the note is used up");
    }

    #[test]
    fn a_game_the_user_paused_is_left_alone() {
        let mut s = Suspension::default();
        assert!(!s.suspend(true), "already paused: nothing to do");
        assert!(!s.resume(), "the user's pause, not the suspend's");
    }

    #[test]
    fn a_second_suspend_keeps_the_first_ones_note() {
        let mut s = Suspension::default();
        assert!(s.suspend(false));
        // The game is paused now (by us); a second suspend must not call it the user's.
        assert!(!s.suspend(true));
        assert!(s.resume());
    }

    #[test]
    fn the_autosave_slot_is_a_plain_file_name() {
        assert_eq!(autosave_file(), "Autosave (App)");
    }
}
