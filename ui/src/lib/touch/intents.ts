import type { Bridge } from '../bridge';
import type { Action } from '../protocol/Action';

let seq = 0;

/** Sends what a tap on the widget `id` means (`FromUi::Intent`, docs/webui.md "Intents"); Rust
 * checks it and answers `rejected` if it cannot apply it. The touch skin never sends pixels for
 * a widget. */
export function intent(bridge: Bridge, id: string, action: Action): void {
  bridge.unlockAudio?.();
  bridge.send({ t: 'intent', seq: ++seq, id, action });
}

export const leftClick = { t: 'click', button: 'left', mods: { shift: false, ctrl: false, alt: false, meta: false } } as const satisfies Action;

/** A click with Ctrl held: on a production icon an endless order (docs/ui.md "Production"). */
export const ctrlClick = { t: 'click', button: 'left', mods: { shift: false, ctrl: true, alt: false, meta: false } } as const satisfies Action;

/** Asks the game to go back (`FromUi::Back`): it presses the screen's own back or cancel widget, as its Escape
 * key does, and answers `rejected` if the screen shows none. */
export function goBack(bridge: Bridge): void {
  bridge.unlockAudio?.();
  bridge.send({ t: 'back', seq: ++seq });
}
