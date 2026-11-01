import { describe, expect, it } from 'vitest';
import { gridColumns, portrait, resolveSkin, sizeClass } from './skin';

const vp = (width: number, height: number, coarse = false) => ({ width, height, coarse });

describe('size classes', () => {
  it('follow the short side', () => {
    expect(sizeClass(vp(390, 844))).toBe('compact');
    expect(sizeClass(vp(844, 390))).toBe('compact');
    expect(sizeClass(vp(499, 2000))).toBe('compact');
    expect(sizeClass(vp(500, 900))).toBe('medium');
    expect(sizeClass(vp(767, 1024))).toBe('medium');
    expect(sizeClass(vp(768, 1024))).toBe('expanded');
    expect(sizeClass(vp(820, 1180))).toBe('expanded');
  });

  it('know the orientation and the grid', () => {
    expect(portrait(vp(390, 844))).toBe(true);
    expect(portrait(vp(844, 390))).toBe(false);
    expect([gridColumns('compact'), gridColumns('medium'), gridColumns('expanded')]).toEqual([4, 5, 6]);
  });
});

describe('the skin', () => {
  it('is the classic one on a desktop and the touch one elsewhere with auto', () => {
    expect(resolveSkin('auto', vp(1920, 1080))).toBe('classic');
    expect(resolveSkin('auto', vp(1024, 768))).toBe('classic');
    expect(resolveSkin('auto', vp(1920, 1080, true))).toBe('touch');
    expect(resolveSkin('auto', vp(820, 1180, true))).toBe('touch');
    expect(resolveSkin('auto', vp(390, 844))).toBe('touch');
    expect(resolveSkin('auto', vp(600, 400))).toBe('touch');
  });

  it('obeys an explicit setting', () => {
    expect(resolveSkin('classic', vp(390, 844, true))).toBe('classic');
    expect(resolveSkin('touch', vp(1920, 1080))).toBe('touch');
  });
});
