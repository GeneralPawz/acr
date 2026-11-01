import { describe, expect, it } from 'vitest';
import type { WidgetView } from '../protocol/WidgetView';
import { DEFAULT_FONT, fitScale, fontOf, sheetItems, words } from './sheet';

const widget = (id: string, kind: WidgetView['kind'], rect: [number, number, number, number], extra: Partial<WidgetView> = {}): WidgetView => ({
  id,
  kind,
  rect,
  enabled: true,
  shown: true,
  hover: false,
  parts: [],
  ...extra,
});

describe('sheet', () => {
  it('keeps the widgets with something to say, in reading order', () => {
    const items = sheetItems([
      widget('S/0', 'picture', [0, 0, 10, 10]),
      widget('S/1', 'button', [10, 300, 5, 5], { label: 'OK' }),
      widget('S/2', 'button', [10, 100, 5, 5]),
      widget('S/7', 'button', [10, 310, 5, 5], { key: 'start' }),
      widget('S/3', 'text', [200, 50, 5, 5], { label: 'Right' }),
      widget('S/4', 'text', [10, 50, 5, 5], { label: 'Left' }),
      widget('S/5', 'list', [0, 120, 5, 5], { value: { t: 'list', rows: ['a'], selected: null } }),
      widget('S/dropdown', 'combo', [0, 0, 0, 0], { value: { t: 'combo', options: [], selected: 0, open: true } }),
      widget('S/6', 'scrollbar', [0, 0, 5, 5]),
      widget('S/8', 'button', [10, 400, 5, 5], { label: 'Hidden', shown: false }),
    ]);
    expect(items.map((w) => w.id)).toEqual(['S/4', 'S/3', 'S/5', 'S/1', 'S/7']);
  });

  it('takes the font a widget drew its text with', () => {
    const text = { t: 'text', font: 'YellowFont', set: 2, text: 'x', x: 0, y: 0, fade: 0, clip: null } as const;
    expect(fontOf(widget('S/0', 'button', [0, 0, 1, 1], { parts: [text] }))).toEqual({ font: 'YellowFont', set: 2 });
    expect(fontOf(widget('S/0', 'button', [0, 0, 1, 1]))).toEqual(DEFAULT_FONT);
  });

  it('fits a text to its slot', () => {
    expect(fitScale(100, 20, 1.5)).toBe(1.5);
    expect(fitScale(100, 40, 1.5, 0, 30)).toBe(0.75);
    expect(fitScale(100, 20, 1.5, 120)).toBe(1);
    expect(fitScale(100, 20, 1.5, 149)).toBe(1);
    expect(fitScale(100, 20, 1.5, 150)).toBe(1.5);
    expect(fitScale(100, 20, 1.5, 70)).toBeCloseTo(0.7);
  });

  it('splits a text into words', () => {
    expect(words(' a  b\nc\r\n')).toEqual(['a', 'b', 'c']);
  });
});
