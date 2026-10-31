import type { Button } from './protocol/Button';
import type { FromUi } from './protocol/FromUi';
import type { Mods } from './protocol/Mods';

export function modsOf(e: { shiftKey: boolean; ctrlKey: boolean; altKey: boolean; metaKey: boolean }): Mods {
  return { shift: e.shiftKey, ctrl: e.ctrlKey, alt: e.altKey, meta: e.metaKey };
}

export function buttonOf(button: number): Button {
  return button === 1 ? 'middle' : button === 2 ? 'right' : 'left';
}

/** Wheel notches, positive away from the user (up), as the game expects: the DOM's
 * `deltaY` is positive downwards, in pixels, lines or pages. */
export function wheelLines(e: { deltaY: number; deltaMode: number }): number {
  const unit = e.deltaMode === 1 ? 1 : e.deltaMode === 2 ? 10 : 1 / 40;
  return -e.deltaY * unit;
}

/** Keys the browser keeps for itself (reload); everything else belongs to the game. */
export function browserKey(e: { key: string; ctrlKey: boolean; metaKey: boolean }): boolean {
  return e.key === 'F5' || ((e.ctrlKey || e.metaKey) && /^[a-z]$/i.test(e.key) && e.key !== 'f' && e.key !== 'p');
}

/** Pointer moves of a frame are sent once, with the last position; any other pointer
 * message flushes the pending move first so the order is kept. */
export class MoveCoalescer {
  #pending: Extract<FromUi, { t: 'pointer' }> | null = null;
  #scheduled = false;
  readonly #send: (m: FromUi) => void;
  readonly #frame: (run: () => void) => void;

  constructor(send: (m: FromUi) => void, frame: (run: () => void) => void = (run) => requestAnimationFrame(run)) {
    this.#send = send;
    this.#frame = frame;
  }

  move(message: Extract<FromUi, { t: 'pointer' }>): void {
    this.#pending = message;
    if (!this.#scheduled) {
      this.#scheduled = true;
      this.#frame(() => {
        this.#scheduled = false;
        this.flush();
      });
    }
  }

  flush(): void {
    if (this.#pending) {
      this.#send(this.#pending);
      this.#pending = null;
    }
  }

  send(message: FromUi): void {
    this.flush();
    this.#send(message);
  }
}
