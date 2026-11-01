import type { Skin } from './protocol/Skin';
import type { SkinSetting } from './protocol/SkinSetting';

/** Size classes by the viewport's short side in CSS pixels (docs/webui.md "Touch skin"). */
export type SizeClass = 'compact' | 'medium' | 'expanded';

export interface Viewport {
  /** CSS pixels. */
  width: number;
  height: number;
  /** `(pointer: coarse)`: the primary pointer is a finger. */
  coarse: boolean;
}

export function sizeClass({ width, height }: Pick<Viewport, 'width' | 'height'>): SizeClass {
  const short = Math.min(width, height);
  return short < 500 ? 'compact' : short < 768 ? 'medium' : 'expanded';
}

export function portrait({ width, height }: Pick<Viewport, 'width' | 'height'>): boolean {
  return height > width;
}

/** The skin for `setting`: `auto` keeps the classic skin on an expanded viewport with a fine
 * pointer (what a desktop has always had) and takes the touch skin everywhere else. */
export function resolveSkin(setting: SkinSetting, viewport: Viewport): Skin {
  if (setting !== 'auto') return setting;
  return sizeClass(viewport) === 'expanded' && !viewport.coarse ? 'classic' : 'touch';
}

/** Columns of the command grid: 4 on a compact viewport, 5 on a medium one, 6 on an expanded one. */
export function gridColumns(size: SizeClass): number {
  return size === 'compact' ? 4 : size === 'medium' ? 5 : 6;
}
