/** Screens (`Screen::name`) the UI draws itself. A screen joins when its parity check passes
 * (`cargo xtask ui-diff`, docs/webui.md); the engine draws all others. */
export const SCREENS: readonly string[] = ['MainMenu', 'PlayerName', 'Options', 'Campaigns', 'SingleMission'];
