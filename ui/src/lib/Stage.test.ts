import { render } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import type { Bridge } from './bridge';
import { KEY } from './context';
import { FontCache } from './fonts';
import Ui from './Ui.svelte';
import { UiState } from './state.svelte';
import type { FromUi } from './protocol/FromUi';

const bridge = (sent: FromUi[]): Bridge => ({
  send: (m) => sent.push(m),
  onMessage: () => {},
  assetUrl: async (path) => `blob:${path}`,
  assetBytes: async () => new TextEncoder().encode('{"height":1,"glyphs":{}}'),
});

function mountWith(model: UiState, b: Bridge) {
  return render(Ui, { context: new Map([[KEY, { bridge: b, fonts: new FontCache(b), model }]]) });
}

const canvas = { scale: 2, origin: [100, 0] as [number, number], size: [1024, 768] as [number, number] };

describe('Ui', () => {
  it('draws a claimed screen on a scaled stage and nothing for an engine-drawn one', () => {
    const sent: FromUi[] = [];
    const b = bridge(sent);
    const model = new UiState(b);
    const { container, unmount } = mountWith(model, b);
    model.receive({
      t: 'view',
      rev: 1,
      mode: 'menus',
      stage: { menu: canvas, game: canvas },
      screen: {
        name: 'MainMenu',
        stack: ['MainMenu'],
        in_game: false,
        busy: false,
        engine_drawn: false,
        hint: null,
        widgets: {
          t: 'full',
          widgets: [
            {
              id: 'MainMenu/0',
              kind: 'picture',
              rect: [0, 0, 5, 5],
              enabled: true,
              hover: false,
              parts: [{ t: 'fill', rgba: [1, 2, 3, 255], rect: [0, 0, 5, 5], clip: null }],
            },
          ],
        },
      },
    });
    return Promise.resolve().then(() => {
      const stage = container.querySelector<HTMLElement>('.stage');
      expect(stage?.style.transform).toBe('scale(2)');
      expect(stage?.style.left).toBe('100px');
      expect(container.querySelector('[data-acr-id="MainMenu/0"]')).not.toBeNull();
      model.receive({
        t: 'view',
        rev: 2,
        screen: { name: 'Options', stack: ['MainMenu', 'Options'], in_game: false, busy: false, engine_drawn: true, hint: null, widgets: { t: 'full', widgets: [] } },
      });
      return Promise.resolve().then(() => {
        expect(container.querySelector('.stage')).toBeNull();
        expect(container.querySelector('[data-acr-viewport]')).not.toBeNull();
        unmount();
      });
    });
  });

  it('keeps the claimed loading screen while the game loads and hides it in the game', async () => {
    const b = bridge([]);
    const model = new UiState(b);
    const { container } = mountWith(model, b);
    const screen = (engine_drawn: boolean) => ({
      name: 'Loading',
      stack: ['Loading'],
      in_game: false,
      busy: false,
      engine_drawn,
      hint: null,
      widgets: { t: 'full' as const, widgets: [] },
    });
    model.receive({ t: 'view', rev: 1, mode: 'loading', loading: 0.4, stage: { menu: canvas, game: canvas }, screen: screen(false) });
    await Promise.resolve();
    expect(container.querySelector('[data-acr-screen="Loading"]')).not.toBeNull();
    model.receive({ t: 'view', rev: 2, mode: 'game' });
    await Promise.resolve();
    expect(container.querySelector('.stage')).toBeNull();
  });

  it('draws a dialog over the running game and nothing for a menu screen or an engine-drawn dialog', async () => {
    const b = bridge([]);
    const model = new UiState(b);
    const { container } = mountWith(model, b);
    const screen = (name: string, in_game: boolean, engine_drawn: boolean) => ({
      name,
      stack: [name],
      in_game,
      busy: false,
      engine_drawn,
      hint: null,
      widgets: { t: 'full' as const, widgets: [] },
    });
    const view = async (rev: number, s: ReturnType<typeof screen>) => {
      model.receive({ t: 'view', rev, mode: rev === 1 ? 'game' : undefined, stage: rev === 1 ? { menu: canvas, game: canvas } : undefined, screen: s });
      await Promise.resolve();
    };
    await view(1, screen('GameMenu', true, false));
    expect(container.querySelector('[data-acr-screen="GameMenu"]')).not.toBeNull();
    // The viewport under it still forwards the input the dialog does not use.
    expect(container.querySelector('[data-acr-viewport]')).not.toBeNull();
    await view(2, screen('GameMenu', true, true));
    expect(container.querySelector('.stage')).toBeNull();
    // No screen of the menus lies over the running game.
    await view(3, screen('MainMenu', false, false));
    expect(container.querySelector('.stage')).toBeNull();
  });

  it('draws the interface groups over the game canvas, scaled about their anchor', async () => {
    const b = bridge([]);
    const model = new UiState(b);
    const { container } = mountWith(model, b);
    const fill = { t: 'fill' as const, rgba: [1, 2, 3, 255] as [number, number, number, number], rect: [0, 0, 5, 5] as [number, number, number, number], clip: null };
    model.receive({
      t: 'view',
      rev: 1,
      mode: 'game',
      stage: { menu: canvas, game: canvas },
      hud: {
        groups: [
          { name: 'hint', layer: 3, anchor: [30, 40], scale: 2, parts: [fill] },
          { name: 'top_bar', layer: 0, anchor: [0, 0], scale: 1, parts: [fill] },
        ],
        removed: [],
      },
    });
    await Promise.resolve();
    const groups = [...container.querySelectorAll<HTMLElement>('[data-acr-hud-group]')];
    expect(groups.map((g) => g.dataset.acrHudGroup)).toEqual(['top_bar', 'hint']);
    expect(groups[0].style.transform).toBe('');
    expect(groups[1].style.transform).toBe('scale(2)');
    expect(groups[1].style.transformOrigin).toBe('30px 40px');
    // No interface outside the running game.
    model.receive({ t: 'view', rev: 2, mode: 'menus' });
    await Promise.resolve();
    expect(container.querySelector('[data-acr-hud]')).toBeNull();
  });

  it('forwards pointer input in physical pixels', async () => {
    const sent: FromUi[] = [];
    const b = bridge(sent);
    const model = new UiState(b);
    const { container } = mountWith(model, b);
    const viewport = container.querySelector('[data-acr-viewport]') as HTMLElement;
    // jsdom has no pointer capture.
    viewport.setPointerCapture = () => {};
    viewport.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, clientX: 10, clientY: 20, button: 0 }));
    expect(sent.at(-1)).toMatchObject({ t: 'pointer', phase: 'down', x: 10, y: 20, button: 'left' });
  });
});
