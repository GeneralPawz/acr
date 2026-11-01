import { fireEvent, render } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { Bridge } from '../bridge';
import { KEY } from '../context';
import { FontCache } from '../fonts';
import type { FromUi } from '../protocol/FromUi';
import { UiState } from '../state.svelte';
import TouchMenu from './TouchMenu.svelte';

const bridge = (sent: FromUi[]): Bridge => ({
  send: (m) => sent.push(m),
  onMessage: () => {},
  assetUrl: async (path) => `blob:${path}`,
  assetBytes: async () => new TextEncoder().encode('{"height":1,"glyphs":{}}'),
});

describe('TouchMenu', () => {
  // jsdom has no layout: a fitted label only needs the observer to exist.
  vi.stubGlobal('ResizeObserver', class { observe() {} unobserve() {} disconnect() {} });

  it('sends the intents of its entries and never a pixel', async () => {
    const sent: FromUi[] = [];
    const b = bridge(sent);
    const model = new UiState(b);
    model.receive({
      t: 'view',
      rev: 1,
      mode: 'menus',
      screen: {
        name: 'Options',
        stack: ['Options'],
        in_game: false,
        busy: false,
        engine_drawn: false,
        hint: null,
        widgets: {
          t: 'full',
          widgets: [
            { id: 'Options/0', key: 'ok', kind: 'button', rect: [0, 300, 9, 9], enabled: true, shown: true, hover: false, label: 'OK', parts: [] },
            { id: 'Options/1', key: 'volume', kind: 'slider', rect: [0, 100, 9, 9], enabled: true, shown: true, hover: false, value: { t: 'range', value: 3, max: 10 }, parts: [] },
            { id: 'Options/2', key: 'saves', kind: 'list', rect: [0, 200, 9, 9], enabled: true, shown: true, hover: false, value: { t: 'list', rows: ['a', 'b'], selected: null }, parts: [] },
          ],
        },
      },
    });
    const { container } = render(TouchMenu, { context: new Map([[KEY, { bridge: b, fonts: new FontCache(b), model }]]) });
    await fireEvent.click(container.querySelector('[data-acr-key="ok"]')!);
    const rows = container.querySelectorAll('[role=option]');
    await fireEvent.click(rows[1]);
    const range = container.querySelector<HTMLInputElement>('input[type=range]')!;
    range.value = '7';
    await fireEvent.change(range);
    expect(sent.map((m) => (m.t === 'intent' ? [m.id, m.action.t] : m.t))).toEqual([
      ['Options/0', 'click'],
      ['Options/2', 'select_row'],
      ['Options/1', 'set_value'],
    ]);
    expect(sent[2]).toMatchObject({ action: { t: 'set_value', value: 7 } });
    // The sheet lists them top to bottom.
    expect([...container.querySelectorAll('[data-acr-id]')].map((e) => (e as HTMLElement).dataset.acrId)).toEqual(['Options/1', 'Options/2', 'Options/0']);
  });
});
