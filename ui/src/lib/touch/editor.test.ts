import { fireEvent, render } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Bridge } from '../bridge';
import { KEY } from '../context';
import { FontCache } from '../fonts';
import type { EditorView } from '../protocol/EditorView';
import { UiState } from '../state.svelte';
import TouchHud from './TouchHud.svelte';

const sent: unknown[] = [];
const bridge: Bridge = {
  send: (m) => void sent.push(m),
  onMessage: () => {},
  assetUrl: async (path) => `blob:${path}`,
  assetBytes: async () => new TextEncoder().encode('{"height":1,"glyphs":{}}'),
};

const button = (id: string, text: string, on = false) => ({ id, text, on });
const view = (over: Partial<EditorView> = {}): EditorView => ({
  title: 'Einheit waehlen',
  toggle: button('editor/battle', 'Schlacht'),
  battle: false,
  status: '',
  armed: null,
  swatches: [0, 1, 2, 3, 4, 5, 6, 7].map((p) => ({ id: `editor/owner/${p}`, label: `Spieler ${p + 1}`, colour: [p * 30, 40, 80] as [number, number, number], team: p === 3 ? 2 : null, on: p === 0, teams: p === 3 })),
  entries: [0, 1, 2].map((i) => button(`editor/nation/${i}`, ['Maya', 'Spanien', 'England'][i], i === 2)),
  army: [button('editor/army/size/0', 'Klein'), button('editor/army/size/1', 'Mittel')],
  groups: [
    {
      header: 'Infanterie',
      cells: [
        { id: 'editor/unit/EngM18(EN)', name: 'Fuesilier', size: [41, 41], parts: [], on: false, levels: true },
        { id: 'editor/ground/gras', name: 'Gras', size: [41, 41], parts: [], on: false, levels: false },
      ],
    },
  ],
  levels: [],
  controls: [
    { t: 'stepper', less: 'editor/count/-', more: 'editor/count/+', text: 'Anzahl: 36' },
    { t: 'line', text: 'Ausbau 0/3: Basis' },
    { t: 'button', id: 'editor/peace', text: 'Kampf: AUS', on: false },
  ],
  ...over,
});

function mount(editor: EditorView | null) {
  const model = new UiState(bridge);
  model.receive({ t: 'view', rev: 1, mode: 'game', touch: { resources: [], icons: [], army: [], paused: false, minimap: false, speed: 50, speed_text: ['Langsam', 'Schnell'] }, editor } as never);
  const out = render(TouchHud, { context: new Map([[KEY, { bridge, fonts: new FontCache(bridge), model }]]) });
  return { ...out, model };
}

const actions = () => sent.map((m) => m as { id: string; action: { t: string; button?: string } });
const drawer = (c: HTMLElement) => c.querySelector('[data-acr-touch=editor]');
const open = async (c: HTMLElement) => {
  await fireEvent.click(c.querySelector('[data-acr-id="bar/editor"]')!);
  return drawer(c)!;
};
const drag = async (el: Element, from: [number, number], to: [number, number]) => {
  await fireEvent.pointerDown(el, { pointerId: 3, clientX: from[0], clientY: from[1] });
  await fireEvent.pointerMove(el, { pointerId: 3, clientX: to[0], clientY: to[1] });
  await fireEvent.pointerUp(el, { pointerId: 3, clientX: to[0], clientY: to[1] });
};

describe('the test editor drawer', () => {
  vi.stubGlobal('ResizeObserver', class { observe() {} unobserve() {} disconnect() {} });
  // jsdom has no Web Animations: a transition runs out as a timer.
  Element.prototype.animate = function () {
    const animation = {
      currentTime: 0,
      cancel() {},
      finish() {},
      set onfinish(done: () => void) {
        setTimeout(done, 170);
      },
    };
    return animation as unknown as Animation;
  };
  beforeEach(() => {
    vi.useFakeTimers();
    sent.length = 0;
  });
  afterEach(() => vi.useRealTimers());

  it('has a bar button only in an editor game, first among the buttons', () => {
    const none = mount(null);
    expect(none.container.querySelector('[data-acr-id="bar/editor"]')).toBeNull();
    expect(none.container.querySelector('[data-acr-touch=edge]')).toBeNull();
    none.unmount();
    const some = mount(view());
    const labels = [...some.container.querySelectorAll('header.top button')].map((b) => b.getAttribute('aria-label'));
    expect(labels[0]).toBe('editor');
    expect(some.container.querySelector('[data-acr-touch=edge]')).not.toBeNull();
    expect(drawer(some.container)).toBeNull();
  });

  it('opens with the bar button, and again closed with it or with the cross', async () => {
    const { container } = mount(view());
    expect(await open(container)).not.toBeNull();
    expect(container.querySelector('[data-acr-touch=edge]')).toBeNull();
    await fireEvent.click(container.querySelector('[data-acr-id="bar/editor"]')!);
    await vi.advanceTimersByTimeAsync(400);
    expect(drawer(container)).toBeNull();
    await open(container);
    await fireEvent.click(container.querySelector('[data-acr-id="drawer/close"]')!);
    await vi.advanceTimersByTimeAsync(400);
    expect(drawer(container)).toBeNull();
    expect(sent).toEqual([]);
  });

  it('opens with a swipe left from the edge strip, and not with a tap or a vertical drag', async () => {
    const { container } = mount(view());
    const edge = container.querySelector('[data-acr-touch=edge]')!;
    await drag(edge, [380, 300], [380, 300]);
    await drag(edge, [380, 300], [378, 380]);
    expect(drawer(container)).toBeNull();
    await drag(edge, [385, 300], [340, 305]);
    expect(drawer(container)).not.toBeNull();
    expect(sent).toEqual([]);
  });

  it('sends the ids the view names: a cell tap is a left click, a long press a right one', async () => {
    const { container } = mount(view());
    await open(container);
    const unit = container.querySelector('[data-acr-id="editor/unit/EngM18(EN)"]')!;
    await fireEvent.pointerDown(unit);
    vi.advanceTimersByTime(100);
    await fireEvent.pointerUp(unit);
    await fireEvent.click(unit);
    expect(actions()).toMatchObject([{ id: 'editor/unit/EngM18(EN)', action: { t: 'click', button: 'left' } }]);
    sent.length = 0;
    await fireEvent.pointerDown(unit);
    vi.advanceTimersByTime(500);
    await fireEvent.pointerUp(unit);
    await fireEvent.click(unit);
    expect(actions()).toMatchObject([{ id: 'editor/unit/EngM18(EN)', action: { t: 'click', button: 'right' } }]);
    // A cell without levels shows its name instead.
    sent.length = 0;
    const ground = container.querySelector('[data-acr-id="editor/ground/gras"]')!;
    await fireEvent.pointerDown(ground);
    vi.advanceTimersByTime(500);
    await fireEvent.pointerUp(ground);
    await fireEvent.click(ground);
    expect(sent).toEqual([]);
    expect(container.querySelector('[role=status]')!.textContent).toContain('Gras');
  });

  it('sends the colours (a long press cycles the team where it counts), the steppers and a control', async () => {
    const { container } = mount(view());
    await open(container);
    await fireEvent.click(container.querySelector('[data-acr-id="editor/owner/1"]')!);
    const team = container.querySelector('[data-acr-id="editor/owner/3"]')!;
    await fireEvent.pointerDown(team);
    vi.advanceTimersByTime(500);
    await fireEvent.pointerUp(team);
    await fireEvent.click(team);
    const plain = container.querySelector('[data-acr-id="editor/owner/1"]')!;
    await fireEvent.pointerDown(plain);
    vi.advanceTimersByTime(500);
    await fireEvent.pointerUp(plain);
    await fireEvent.click(plain);
    await fireEvent.click(container.querySelector('[data-acr-id="editor/count/+"]')!);
    await fireEvent.click(container.querySelector('[data-acr-id="editor/peace"]')!);
    expect(actions().map((a) => [a.id, a.action.button])).toEqual([
      ['editor/owner/1', 'left'],
      ['editor/owner/3', 'right'],
      ['editor/owner/1', 'left'],
      ['editor/count/+', 'left'],
      ['editor/peace', 'left'],
    ]);
  });

  it('picks a nation from the list over the drawer', async () => {
    const { container, getByRole } = mount(view());
    await open(container);
    await fireEvent.click(container.querySelector('[data-acr-id="drawer/nations"]')!);
    const list = getByRole('listbox');
    expect(list.querySelectorAll('button').length).toBe(3);
    await fireEvent.click(list.querySelector('[data-acr-id="editor/nation/1"]')!);
    expect(actions()).toMatchObject([{ id: 'editor/nation/1', action: { t: 'click', button: 'left' } }]);
    expect(container.querySelector('[role=listbox]')).toBeNull();
  });

  it('shows the levels over the drawer, a row picks one and the scrim closes the list', async () => {
    const { container, model } = mount(view({ levels: [button('editor/level/0', 'Basis', true), button('editor/level/1', 'Spezialmusketen')] }));
    await open(container);
    await fireEvent.click(container.querySelector('[data-acr-id="editor/level/1"]')!);
    await fireEvent.click(container.querySelector('[data-acr-id="editor/close"]')!);
    expect(actions().map((a) => a.id)).toEqual(['editor/level/1', 'editor/close']);
    model.editor = view();
    await vi.advanceTimersByTimeAsync(10);
    expect(container.querySelector('[data-acr-id="editor/level/1"]')).toBeNull();
  });

  it('closes the drawer after an army size or Aufstellen, but not after a cell', async () => {
    const { container } = mount(view());
    await open(container);
    await fireEvent.click(container.querySelector('[data-acr-id="editor/unit/EngM18(EN)"]')!);
    expect(drawer(container)).not.toBeNull();
    await fireEvent.click(container.querySelector('[data-acr-id="editor/army/size/1"]')!);
    await vi.advanceTimersByTimeAsync(400);
    expect(drawer(container)).toBeNull();
    expect(actions().map((a) => a.id)).toEqual(['editor/unit/EngM18(EN)', 'editor/army/size/1']);
  });

  it('closes with a swipe to the right and swallows the click that follows it', async () => {
    const { container } = mount(view());
    const el = await open(container);
    const unit = container.querySelector('[data-acr-id="editor/unit/EngM18(EN)"]')!;
    await fireEvent.pointerDown(unit, { pointerId: 3, clientX: 100, clientY: 300 });
    await fireEvent.pointerMove(unit, { pointerId: 3, clientX: 190, clientY: 304 });
    await fireEvent.pointerUp(unit, { pointerId: 3, clientX: 190, clientY: 304 });
    await fireEvent.click(unit);
    await vi.advanceTimersByTimeAsync(400);
    expect(el.isConnected).toBe(false);
    expect(sent).toEqual([]);
  });

  it('shows what a tap on the map places when closed: its name opens the drawer, the cross stops placing', async () => {
    const { container, model } = mount(view({ armed: 'Typ: Fuesilier (EngM18(EN))' }));
    const chip = container.querySelector('[data-acr-touch=placing]')!;
    expect(chip.textContent).toContain('Fuesilier');
    await fireEvent.click(container.querySelector('[data-acr-id="editor/stop"]')!);
    expect(actions()).toMatchObject([{ id: 'editor/stop', action: { t: 'click', button: 'left' } }]);
    await fireEvent.click(chip.querySelector('.what')!);
    expect(drawer(container)).not.toBeNull();
    expect(container.querySelector('[data-acr-touch=placing]')).toBeNull();
    model.editor = null;
    await vi.advanceTimersByTimeAsync(400);
    expect(drawer(container)).toBeNull();
    expect(container.querySelector('[data-acr-id="bar/editor"]')).toBeNull();
  });

  it('answers Escape before the game does: the level list first, then the drawer', async () => {
    const { container, model } = mount(view({ levels: [button('editor/level/0', 'Basis')] }));
    const reached = vi.fn();
    window.addEventListener('keydown', reached);
    await open(container);
    await fireEvent.keyDown(window, { key: 'Escape' });
    expect(actions().map((a) => a.id)).toEqual(['editor/close']);
    expect(drawer(container)).not.toBeNull();
    model.editor = view();
    await vi.advanceTimersByTimeAsync(10);
    await fireEvent.keyDown(window, { key: 'Escape' });
    await vi.advanceTimersByTimeAsync(400);
    expect(drawer(container)).toBeNull();
    expect(reached).not.toHaveBeenCalled();
    // With the drawer shut the key is the game's.
    await fireEvent.keyDown(window, { key: 'Escape' });
    expect(reached).toHaveBeenCalledOnce();
    window.removeEventListener('keydown', reached);
  });
});
