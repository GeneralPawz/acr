import { describe, expect, it } from 'vitest';
import { applyWidgets, isSnapshot } from './patch';
import type { WidgetView } from './protocol/WidgetView';

const widget = (id: string, hover = false): WidgetView => ({
  id,
  kind: 'button',
  rect: [0, 0, 10, 10],
  enabled: true,
  hover,
  parts: [],
});

describe('applyWidgets', () => {
  it('replaces everything with a full patch', () => {
    const next = [widget('a')];
    expect(applyWidgets([widget('x')], { t: 'full', widgets: next })).toBe(next);
  });

  it('upserts in place and keeps the identity of unchanged widgets', () => {
    const [a, b] = [widget('a'), widget('b')];
    const b2 = widget('b', true);
    const out = applyWidgets([a, b], { t: 'changed', upsert: [b2], order: null });
    expect(out).toEqual([a, b2]);
    expect(out[0]).toBe(a);
  });

  it('appends new widgets and follows a new order', () => {
    const [a, b, c] = [widget('a'), widget('b'), widget('c')];
    expect(applyWidgets([a, b], { t: 'changed', upsert: [c], order: null })).toEqual([a, b, c]);
    expect(applyWidgets([a, b], { t: 'changed', upsert: [c], order: ['c', 'a'] })).toEqual([c, a]);
  });

  it('fails on an order that names a widget nobody sent', () => {
    expect(() => applyWidgets([], { t: 'changed', upsert: [], order: ['ghost'] })).toThrow(/ghost/);
  });
});

describe('isSnapshot', () => {
  it('needs every topic', () => {
    const screen = {
      name: 'MainMenu',
      stack: ['MainMenu'],
      in_game: false,
      busy: false,
      engine_drawn: false,
      hint: null,
      widgets: { t: 'full', widgets: [] },
    } as const;
    expect(isSnapshot({ rev: 5, mode: 'menus', screen, cursor: null, loading: null })).toBe(true);
    expect(isSnapshot({ rev: 5, mode: 'menus', screen })).toBe(false);
    expect(isSnapshot({ rev: 5, screen: { ...screen, widgets: { t: 'changed', upsert: [], order: null } } })).toBe(false);
  });
});
