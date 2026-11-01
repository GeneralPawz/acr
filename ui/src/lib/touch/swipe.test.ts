import { describe, expect, it } from 'vitest';
import { AXIS, CLOSE_DX, OPEN_DX, classify, closesDrawer } from './swipe';

const at = (x: number, y: number) => ({ x, y });

describe('drawer swipes', () => {
  it('opens on a leftward drag of the open distance along the axis', () => {
    expect(classify(at(380, 300), at(380 - OPEN_DX + 1, 300), 'open')).toBe('none');
    expect(classify(at(380, 300), at(380 - OPEN_DX, 300), 'open')).toBe('open');
    expect(classify(at(380, 300), at(300, 300 + 40), 'open')).toBe('open');
  });

  it('does not open on a drag to the right, or one too slanted', () => {
    expect(classify(at(380, 300), at(420, 300), 'open')).toBe('none');
    // 20 across, 24 along: along < AXIS * across
    expect(classify(at(380, 300), at(380 - OPEN_DX, 300 + 20), 'open')).toBe('none');
    expect(AXIS).toBeGreaterThan(1);
  });

  it('cancels once the drag has gone vertical', () => {
    expect(classify(at(380, 300), at(378, 300 + OPEN_DX + 1), 'open')).toBe('cancel');
    expect(classify(at(100, 300), at(98, 300 - OPEN_DX - 1), 'close')).toBe('cancel');
  });

  it('closes on a rightward drag of the close distance, which is longer', () => {
    expect(CLOSE_DX).toBeGreaterThan(OPEN_DX);
    expect(classify(at(100, 300), at(100 + CLOSE_DX - 1, 300), 'close')).toBe('none');
    expect(classify(at(100, 300), at(100 + CLOSE_DX, 310), 'close')).toBe('close');
    expect(classify(at(100, 300), at(40, 300), 'close')).toBe('none');
  });

  it('names the ids that close the drawer', () => {
    expect(closesDrawer('editor/army/size/1')).toBe(true);
    expect(closesDrawer('editor/battle/go')).toBe(true);
    expect(closesDrawer('editor/unit/EngM18(EN)')).toBe(false);
    expect(closesDrawer('editor/battle/fight')).toBe(false);
  });
});
