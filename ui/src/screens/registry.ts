/** Screens (`Screen::name`) the UI draws itself. A screen joins when its parity check passes
 * (`cargo xtask ui-diff`, docs/webui.md); the engine draws all others. */
export const SCREENS: readonly string[] = ['MainMenu', 'PlayerName', 'Options', 'Campaigns', 'SingleMission', 'LoadGame', 'RandomMap', 'Placeholder', 'Loading', 'GameMenu', 'GameOptions', 'SaveGameDialog', 'MessageWindow', 'HistoryWindow'];

/** Parts of the in-game interface (`webui::HUD_PARTS`) the UI draws itself, over the game canvas. A part
 * joins when its parity check passes (docs/webui.md). The cursor is the CSS cursor of every mode. */
export const HUD: readonly string[] = ['TopBar', 'DropDown'];
