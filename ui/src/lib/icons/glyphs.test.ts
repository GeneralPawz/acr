// @vitest-environment jsdom
import { render } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import Glyph from './Glyph.svelte';
import { GLYPHS, glyph } from './glyphs';

/** Every name `hud/touch_skin.rs` `glyph` can send, and the bar's own. */
const SENT = [
  'stand-ground', 'advance', 'attack-move', 'patrol', 'attack-ground', 'mortar-zone', 'mortar-clear', 'mortar-fire',
  'mortar-cease', 'aim-guide', 'hold-fire', 'open-fire', 'friendly-fire', 'friendly-safe', 'melee-only', 'any-attack',
  'production-stop', 'production-resume', 'canister-fire', 'canister-load', 'fishing-stop', 'fishing-resume',
  'sowing-stop', 'sowing-resume', 'disband', 'group', 'ungroup', 'rally', 'idle-peasant', 'idle-mine', 'army-list',
  'form-smaller', 'form-larger', 'form-fill', 'page-1', 'page-2', 'page-3', 'market-trade', 'market-clear',
  'formation-line', 'formation-column', 'formation-square', 'formation-crescent', 'formation-wedge',
  'formation-cavalry-line',
  ...[1, 2, 3, 4].flatMap((n) => [`amount-plus-${n}`, `amount-minus-${n}`]),
];
const BAR = ['pause', 'play', 'menu', 'map-open', 'map-closed', 'resources-open', 'resources-closed'];

describe('the glyph set', () => {
  it('draws every glyph the engine names and every bar button', () => {
    for (const name of [...SENT, ...BAR]) expect(glyph(name), name).toBeTruthy();
  });

  it('draws nothing the engine does not name', () => {
    expect(Object.keys(GLYPHS).sort()).toEqual([...SENT, ...BAR].sort());
  });

  it('is well-formed SVG throughout', () => {
    for (const [name, art] of Object.entries(GLYPHS)) {
      const doc = new DOMParser().parseFromString(
        `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48">${art}</svg>`,
        'image/svg+xml',
      );
      expect(doc.getElementsByTagName('parsererror').length, name).toBe(0);
      expect(doc.documentElement.children.length, name).toBeGreaterThan(0);
    }
  });

  it('leaves an unknown name to the original picture', () => {
    expect(glyph('no-such-glyph')).toBeUndefined();
    expect(glyph(null)).toBeUndefined();
  });

  it('renders as an svg, with a plaque on request', () => {
    const bare = render(Glyph, { name: 'rally' });
    expect(bare.container.querySelector('svg[data-acr-glyph="rally"] path')).not.toBeNull();
    expect(bare.container.querySelector('.leather')).toBeNull();
    const plaque = render(Glyph, { name: 'rally', plaque: true });
    expect(plaque.container.querySelector('.leather')).not.toBeNull();
  });
});
