//! What the scripts want the player to see: pages, messages, hints, the end of the game.
//!
//! The game UI drains these with [`crate::Scripts::take_events`] after every tick and shows
//! them; the simulation never waits for the UI.

/// One event raised by a mission or AI script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissionEvent {
    /// `ShowPage("#PAGE0")` and friends: a page of the mission text file (picture + text).
    /// The original opens it as a modal window inside the script call, so the game stands
    /// still until it is closed; the UI should pause the game while it shows one.
    Page {
        /// Text id as the script passed it (`#PAGE0`).
        id: String,
        /// Header of the text entry (picture name and a number), empty if the id is unknown.
        header: String,
        /// CP1252 text with `%d`/`%s` parameters already substituted.
        text: Vec<u8>,
    },
    /// A question with OK/No (`AskQuestion`). The original blocks the game until it is
    /// answered; ACR stops running scripts and the simulation (see [`crate::Scripts::waiting`])
    /// until [`crate::Scripts::answer`] is called (1 = OK, 0 = No).
    Question { id: String, header: String, text: Vec<u8> },
    /// `ShowCentralText`: a short message in the middle of the screen for `ticks` ticks.
    CentralText { text: Vec<u8>, ticks: u32 },
    /// `AssignHint`/`AssignHint1`: a hint line at the bottom of the screen for `ticks` frames.
    Hint { text: Vec<u8>, ticks: u32 },
    /// The local player has won (`ShowVictory`).
    Victory,
    /// The local player has lost (`LooseGame`).
    Defeat,
    /// The objectives text (`#BRIEFING` with its `{X}` sections switched by
    /// `EnableMission`/`DisableMission`) changed; see [`crate::Scripts::briefing`].
    Briefing,
    /// `SetStartPoint`: centre the camera on this point (world pixels).
    Camera { x: i32, y: i32 },
    /// `ShowPage`-style multiple choice (`AskMultilineQuestion`): the choices are text lines;
    /// the answer is the 0-based index.
    Choice { id: String, header: String, text: Vec<u8>, choices: Vec<Vec<u8>> },
    /// The script reported an error (`MissErrorMessage`) or crashed in the interpreter.
    Error { title: String, text: String },
}
