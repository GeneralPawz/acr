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

  it('sets the labels as text, repeated title left out, and marks the page switch', () => {
    const b = bridge([]);
    const model = new UiState(b);
    const w = (id: string, key: string, kind: 'text' | 'button', label: string, y: number) =>
      ({ id, key, kind, rect: [0, y, 9, 9], enabled: true, shown: true, hover: false, label, parts: [] }) as never;
    model.receive({
      t: 'view',
      rev: 1,
      mode: 'menus',
      screen: {
        name: 'GameOptions',
        stack: ['GameOptions'],
        in_game: true,
        busy: false,
        engine_drawn: false,
        hint: null,
        widgets: {
          t: 'full',
          widgets: [
            w('G/0', 'title', 'text', 'Einstellungen', 0),
            w('G/1', 'x', 'text', 'Einstellungen', 20),
            w('G/2', 'next_page', 'button', 'Seite 2/6: Grafik \u00bb', 40),
            w('G/3', 'ok', 'button', 'Zur\u00fcck', 60),
          ],
        },
      },
    });
    const { container } = render(TouchMenu, { context: new Map([[KEY, { bridge: b, fonts: new FontCache(b), model }]]) });
    expect(container.querySelector('h1')?.textContent).toBe('Einstellungen');
    expect(container.querySelectorAll('.plain')).toHaveLength(0);
    expect(container.querySelector('[data-acr-key="next_page"]')?.classList.contains('pager')).toBe(true);
    expect(container.querySelector('[data-acr-key="ok"]')?.textContent?.trim()).toBe('Zur\u00fcck');
  });

  it('has a header with the title and a back arrow that asks the game to go back', async () => {
    const sent: FromUi[] = [];
    const b = bridge(sent);
    const model = new UiState(b);
    const w = (id: string, key: string, kind: 'text' | 'button' | 'combo', label: string | undefined, y: number, value?: object) =>
      ({ id, key, kind, rect: [0, y, 9, 9], enabled: true, shown: true, hover: false, label, value, parts: [] }) as never;
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
        back: 'Options/2',
        widgets: {
          t: 'full',
          widgets: [
            w('Options/0', 'title', 'text', 'OPTIONEN ', 0),
            w('Options/1', 'ok', 'button', 'OK', 300),
            w('Options/2', 'cancel', 'button', 'Abbruch', 320),
            w('Options/3', 'res', 'combo', undefined, 100, { t: 'combo', options: ['a', 'b', 'c'], selected: 0, open: false }),
          ],
        },
      },
    });
    const { container } = render(TouchMenu, { context: new Map([[KEY, { bridge: b, fonts: new FontCache(b), model }]]) });
    // The title and the cancel entry are the header's: neither is a sheet entry.
    expect([...container.querySelectorAll('.items [data-acr-id]')].map((e) => (e as HTMLElement).dataset.acrId)).toEqual(['Options/3', 'Options/1']);
    expect(container.querySelector('.title')).not.toBeNull();
    expect(container.querySelector('[data-acr-hit]')).not.toBeNull();
    await fireEvent.click(container.querySelector('[data-acr-back]')!);
    // The drop-down's choices are a list over the sheet, closed by a tap beside it.
    await fireEvent.click(container.querySelector('[data-acr-id="Options/3"]')!);
    expect(container.querySelectorAll('.picker [role=option]')).toHaveLength(3);
    expect(container.querySelector('.picker')!.hasAttribute('data-acr-hit')).toBe(true);
    expect(container.querySelector('main.sheet')!.hasAttribute('data-acr-hit')).toBe(true);
    await fireEvent.click(container.querySelector('.picker')!);
    expect(container.querySelector('.picker')).toBeNull();
    await fireEvent.click(container.querySelector('[data-acr-id="Options/3"]')!);
    await fireEvent.click(container.querySelectorAll('.picker [role=option]')[2]);
    expect(container.querySelector('.picker')).toBeNull();
    expect(sent.map((m) => (m.t === 'intent' ? [m.id, m.action.t] : m.t))).toEqual(['back', ['Options/3', 'select_option']]);
    expect(sent[1]).toMatchObject({ action: { t: 'select_option', option: 2 } });
  });
});
