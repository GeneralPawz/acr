import { render } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { Bridge } from '../bridge';
import { KEY } from '../context';
import { FontCache } from '../fonts';
import { UiState } from '../state.svelte';
import TouchHud from './TouchHud.svelte';

const bridge: Bridge = {
  send: () => {},
  onMessage: () => {},
  assetUrl: async (path) => `blob:${path}`,
  assetBytes: async () => new TextEncoder().encode('{"height":1,"glyphs":{}}'),
};

// The Android host gives the page only the touches inside `data-acr-hit` rectangles.
describe('touch skin hit regions', () => {
  vi.stubGlobal('ResizeObserver', class { observe() {} unobserve() {} disconnect() {} });

  it('marks the HUD pieces that take touches', () => {
    const model = new UiState(bridge);
    model.receive({
      t: 'view',
      rev: 1,
      mode: 'game',
      touch: {
        resources: [],
        icons: [{ id: 'hud/0', role: 'command', size: [40, 40], parts: [], hint: '', disabled: false, selected: false, clickable: true, endless: false }],
        army: [],
        paused: false,
        minimap: true,
      },
    } as never);
    model.hud = [
      { name: 'minimap', layer: 0, anchor: [0, 0], scale: 1, offset: [0, 0], parts: [{ t: 'sprite', url: 'map', x: 0, y: 0, w: 100, h: 100, alpha: 1, clip: null }] },
    ];
    const { container } = render(TouchHud, { context: new Map([[KEY, { bridge, fonts: new FontCache(bridge), model }]]) });
    for (const selector of ['header.top', '.minimap', 'aside.commands']) {
      const el = container.querySelector(selector);
      expect(el, selector).not.toBeNull();
      expect(el!.hasAttribute('data-acr-hit'), selector).toBe(true);
    }
  });
});
