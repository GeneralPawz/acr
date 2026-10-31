import { describe, expect, it } from 'vitest';
import { MoveCoalescer, browserKey, buttonOf, wheelLines } from './input';
import type { FromUi } from './protocol/FromUi';

const mods = { shift: false, ctrl: false, alt: false, meta: false };
const at = (x: number): Extract<FromUi, { t: 'pointer' }> => ({ t: 'pointer', phase: 'move', x, y: 0, button: null, mods });

describe('MoveCoalescer', () => {
  it('sends one move per frame, the last position', () => {
    const sent: FromUi[] = [];
    const frames: (() => void)[] = [];
    const out = new MoveCoalescer((m) => sent.push(m), (run) => frames.push(run));
    out.move(at(1));
    out.move(at(2));
    out.move(at(3));
    expect(frames).toHaveLength(1);
    frames[0]();
    expect(sent).toEqual([at(3)]);
  });

  it('flushes a pending move before another message', () => {
    const sent: FromUi[] = [];
    const out = new MoveCoalescer((m) => sent.push(m), () => {});
    out.move(at(7));
    out.send({ t: 'blur' });
    expect(sent).toEqual([at(7), { t: 'blur' }]);
  });
});

describe('DOM input', () => {
  it('maps buttons and wheel direction', () => {
    expect([0, 1, 2].map(buttonOf)).toEqual(['left', 'middle', 'right']);
    // Scrolling down is negative for the game; 40 pixels are one notch.
    expect(wheelLines({ deltaY: 40, deltaMode: 0 })).toBe(-1);
    expect(wheelLines({ deltaY: -2, deltaMode: 1 })).toBe(2);
  });

  it('keeps browser shortcuts out of the game', () => {
    expect(browserKey({ key: 'F5', ctrlKey: false, metaKey: false })).toBe(true);
    expect(browserKey({ key: 'r', ctrlKey: true, metaKey: false })).toBe(true);
    expect(browserKey({ key: 'f', ctrlKey: true, metaKey: false })).toBe(false);
    expect(browserKey({ key: 'Escape', ctrlKey: false, metaKey: false })).toBe(false);
  });
});
