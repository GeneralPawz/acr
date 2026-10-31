import { describe, expect, it } from 'vitest';
import { layout, type FontMetrics } from './fonts';

const metrics: FontMetrics = {
  height: 10,
  glyphs: {
    A: [0, 0, 6, 8, 0, 1, 7],
    ' ': [0, 0, 0, 0, 0, 0, 4],
    j: [10, 0, 5, 10, -2, 1, 3],
  },
};

describe('layout', () => {
  it('advances the pen and places glyphs from their offsets', () => {
    const { glyphs, width } = layout(metrics, 'A jA');
    expect(width).toBe(7 + 4 + 3 + 7);
    expect(glyphs.map((g) => [g.x, g.y])).toEqual([
      [0, 1],
      [9, 1],
      [14, 1],
    ]);
    expect(glyphs[1]).toMatchObject({ sx: 10, sy: 0, w: 5, h: 10 });
  });

  it('fails on a character the atlas lacks', () => {
    expect(() => layout(metrics, 'AZ')).toThrow(/"Z"/);
  });
});
