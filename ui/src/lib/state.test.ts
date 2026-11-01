import { describe, expect, it } from 'vitest';
import type { Bridge } from './bridge';
import type { FromUi } from './protocol/FromUi';
import type { ScreenPatch } from './protocol/ScreenPatch';
import type { ViewPatch } from './protocol/ViewPatch';
import { UiState } from './state.svelte';

function setup() {
  const sent: FromUi[] = [];
  const bridge: Bridge = {
    send: (m) => sent.push(m),
    onMessage: () => {},
    assetUrl: async (p) => p,
    assetBytes: async () => new Uint8Array(),
  };
  return { sent, state: new UiState(bridge) };
}

const screen = (name = 'MainMenu'): ScreenPatch => ({
  name,
  stack: [name],
  in_game: false,
  busy: false,
  engine_drawn: false,
  hint: null,
  widgets: { t: 'full', widgets: [] },
});

const view = (patch: ViewPatch) => ({ t: 'view' as const, ...patch });

describe('UiState', () => {
  it('leaves the view alone when the game refuses an intent', () => {
    const { state, sent } = setup();
    state.receive({ t: 'hello', protocol: 1, host: 'web', asset_base: '', skin: 'auto' });
    state.receive(view({ rev: 1, mode: 'menus', screen: screen() }));
    state.receive({ t: 'rejected', seq: 3, reason: 'no such widget' });
    state.receive(view({ rev: 2, mode: 'game' }));
    expect(state.mode).toBe('game');
    expect(sent).toEqual([]);
  });

  it('applies patches in order, topic by topic', () => {
    const { state, sent } = setup();
    state.receive({ t: 'hello', protocol: 1, host: 'web', asset_base: '', skin: 'auto' });
    state.receive(view({ rev: 1, mode: 'menus', screen: screen() }));
    state.receive(view({ rev: 2, cursor: { url: 'frame/menu/0/C.GP/0', hotspot: [1, 2] } }));
    expect(state.mode).toBe('menus');
    expect(state.screen?.name).toBe('MainMenu');
    expect(state.cursor?.hotspot).toEqual([1, 2]);
    state.receive(view({ rev: 3, cursor: null }));
    expect(state.cursor).toBeNull();
    expect(state.mode).toBe('menus');
    expect(sent).toEqual([]);
  });

  it('asks for a resync on a gap and waits for a snapshot', () => {
    const { state, sent } = setup();
    state.receive({ t: 'hello', protocol: 1, host: 'web', asset_base: '', skin: 'auto' });
    state.receive(view({ rev: 1, mode: 'menus' }));
    state.receive(view({ rev: 3, mode: 'game' }));
    expect(sent).toEqual([{ t: 'resync' }]);
    expect(state.mode).toBe('menus');
    // Patches that are no snapshot are dropped while waiting.
    state.receive(view({ rev: 4, mode: 'game' }));
    expect(state.mode).toBe('menus');
    state.receive(view({ rev: 5, mode: 'intro', screen: screen(), cursor: null, loading: null }));
    expect(state.mode).toBe('intro');
    state.receive(view({ rev: 6, mode: 'menus' }));
    expect(state.mode).toBe('menus');
    expect(sent).toHaveLength(1);
  });

  it('forgets the interface groups a snapshot does not carry', () => {
    const { state } = setup();
    const group = (name: string) => ({ name, layer: 0, anchor: [0, 0] as [number, number], scale: 1, offset: [0, 0] as [number, number], parts: [] });
    state.receive({ t: 'hello', protocol: 1, host: 'web', asset_base: '', skin: 'auto' });
    state.receive(view({ rev: 1, mode: 'game', hud: { groups: [group('hint'), group('top_bar')], moved: [], removed: [] } }));
    expect(state.hud.map((g) => g.name).sort()).toEqual(['hint', 'top_bar']);
    state.receive(view({ rev: 3, mode: 'game' }));
    state.receive(view({ rev: 4, mode: 'game', screen: screen(), cursor: null, loading: null, hud: { groups: [group('top_bar')], moved: [], removed: [] } }));
    expect(state.hud.map((g) => g.name)).toEqual(['top_bar']);
  });
});
