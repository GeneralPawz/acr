import { describe, expect, it } from 'vitest';
import { NORMAL, STOPS, nearestStop, speedOf, ticksPerSecond } from './speed';

describe('game speed stops', () => {
  it('step from 5 to 100 ticks per second in about even ratios', () => {
    const rates = STOPS.map(ticksPerSecond);
    expect(rates).toEqual([5, 7, 10, 14, 20, 29, 50, 100]);
    for (let i = 1; i < rates.length; i++) expect(rates[i] / rates[i - 1]).toBeLessThan(2.1);
  });

  it('keeps the original default as its middle stop', () => {
    expect(speedOf(NORMAL)).toBe(50);
    expect(ticksPerSecond(50)).toBe(10);
  });

  it('maps a stop to the options slider position and back', () => {
    for (let i = 0; i < STOPS.length; i++) expect(nearestStop(speedOf(i))).toBe(i);
  });

  it('shows the nearest stop for a value between two', () => {
    expect(nearestStop(52)).toBe(NORMAL); // FPSTime 48: 10 ticks/s
    expect(nearestStop(0)).toBe(0);
    expect(nearestStop(100)).toBe(STOPS.length - 1);
    expect(nearestStop(66)).toBe(3); // FPSTime 34 is 14 ticks/s
  });

  it('clamps a stop outside the list', () => {
    expect(speedOf(-3)).toBe(0);
    expect(speedOf(99)).toBe(95);
  });
});
