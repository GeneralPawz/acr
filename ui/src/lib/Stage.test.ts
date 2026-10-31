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
