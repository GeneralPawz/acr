import { fireEvent, render } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Bridge } from '../bridge';
import { KEY } from '../context';
import { FontCache } from '../fonts';
import type { IconView } from '../protocol/IconView';
import { UiState } from '../state.svelte';
import TouchHud from './TouchHud.svelte';

const sent: unknown[] = [];
const bridge: Bridge = {
  send: (m) => void sent.push(m),
  onMessage: () => {},
  assetUrl: async (path) => `blob:${path}`,
  assetBytes: async () => new TextEncoder().encode('{"height":1,"glyphs":{}}'),
};

const icon = (id: string, endless: boolean): IconView => ({
  id,
  role: 'command',
  size: [40, 40],
  parts: [],
  hint: `hint of ${id}`,
  disabled: false,
  selected: false,
  clickable: true,
  endless,
});

function hud(icons: IconView[]) {
  const model = new UiState(bridge);
  model.receive({ t: 'view', rev: 1, mode: 'game', touch: { resources: [], icons, army: [], paused: false, minimap: true } } as never);
  model.hud = [
    { name: 'minimap', layer: 0, anchor: [0, 0], scale: 1, offset: [0, 0], parts: [{ t: 'sprite', url: 'map', x: 0, y: 0, w: 100, h: 100, alpha: 1, clip: null }] },
  ];
  return render(TouchHud, { context: new Map([[KEY, { bridge, fonts: new FontCache(bridge), model }]]) });
}

describe('touch skin icons', () => {
  vi.stubGlobal('ResizeObserver', class { observe() {} unobserve() {} disconnect() {} });
  beforeEach(() => {
    vi.useFakeTimers();
    sent.length = 0;
  });
  afterEach(() => vi.useRealTimers());

  const actions = () => sent.map((m) => (m as { id: string; action: { t: string; mods?: { ctrl: boolean } } }));

  it('a long press on a production icon is a Ctrl+click and shows the hint', async () => {
    const { container, getByRole } = hud([icon('hud/3', true)]);
    const button = container.querySelector('[data-acr-id="hud/3"]')!;
    await fireEvent.pointerDown(button);
    vi.advanceTimersByTime(500);
    await fireEvent.pointerUp(button);
    await fireEvent.click(button);
    expect(actions()).toMatchObject([{ id: 'hud/3', action: { t: 'click', mods: { ctrl: true } } }]);
    expect(getByRole('status').textContent).toContain('hint of hud/3');
  });

  it('a long press on another icon only shows the hint', async () => {
    const { container, getByRole } = hud([icon('hud/4', false)]);
    const button = container.querySelector('[data-acr-id="hud/4"]')!;
    await fireEvent.pointerDown(button);
    vi.advanceTimersByTime(500);
    await fireEvent.pointerUp(button);
    await fireEvent.click(button);
    expect(sent).toEqual([]);
    expect(getByRole('status').textContent).toContain('hint of hud/4');
  });

  it('a tap on a production icon is a plain click', async () => {
    const { container } = hud([icon('hud/3', true)]);
    const button = container.querySelector('[data-acr-id="hud/3"]')!;
    await fireEvent.pointerDown(button);
    vi.advanceTimersByTime(100);
    await fireEvent.pointerUp(button);
    await fireEvent.click(button);
    expect(actions()).toMatchObject([{ id: 'hud/3', action: { t: 'click', mods: { ctrl: false } } }]);
  });

  it('the minimap fold is the last button of the top bar', () => {
    const { container } = hud([]);
    const buttons = [...container.querySelectorAll('header.top button')];
    expect(buttons.at(-1)!.getAttribute('aria-label')).toBe('minimap');
    expect(container.querySelector('.minimap button')).toBeNull();
  });
});
