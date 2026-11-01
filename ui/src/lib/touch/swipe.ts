/** The test editor drawer's swipes (docs/webui.md "Touch skin", "Test editor"): a finger that starts in the
 * edge strip and travels left opens the drawer, one that travels right on the open drawer closes it. */

/** Width of the strip at the right edge that starts the opening swipe, CSS pixels. */
export const EDGE_PX = 20;
/** Leftward travel that opens the drawer. */
export const OPEN_DX = 24;
/** Rightward travel that closes it (more than a scroll's wobble). */
export const CLOSE_DX = 48;
/** The travel must be this many times longer along the axis than across it. */
export const AXIS = 1.5;

export type Swipe = 'none' | 'open' | 'close' | 'cancel';

/** What a drag from `from` to `to` is so far: `open` (leftward) or `close` (rightward) once it has travelled far
 * enough along its axis, `cancel` once it has gone vertical (a scroll: the gesture is over), else `none`. */
export function classify(from: { x: number; y: number }, to: { x: number; y: number }, mode: 'open' | 'close'): Swipe {
  const dx = (mode === 'open' ? from.x - to.x : to.x - from.x);
  const along = Math.max(dx, 0);
  const dy = Math.abs(to.y - from.y);
  const need = mode === 'open' ? OPEN_DX : CLOSE_DX;
  if (along >= need && along >= AXIS * dy) return mode;
  if (dy > OPEN_DX && dy > Math.abs(to.x - from.x)) return 'cancel';
  return 'none';
}

/** Whether a drawer button's id closes the drawer after it: a chosen army size and the battle's "Aufstellen" leave
 * the map free to place on; picking a cell does not (the count and the formation are set after the pick). */
export function closesDrawer(id: string): boolean {
  return id.startsWith('editor/army/size/') || id === 'editor/battle/go';
}
