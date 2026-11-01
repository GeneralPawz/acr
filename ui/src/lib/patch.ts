import type { HudGroup } from './protocol/HudGroup';
import type { HudPatch } from './protocol/HudPatch';
import type { ViewPatch } from './protocol/ViewPatch';
import type { WidgetView } from './protocol/WidgetView';
import type { WidgetsPatch } from './protocol/WidgetsPatch';

/** The widgets after `patch`: unchanged widgets keep their identity. */
export function applyWidgets(current: readonly WidgetView[], patch: WidgetsPatch): WidgetView[] {
  if (patch.t === 'full') return patch.widgets;
  const byId = new Map(current.map((w) => [w.id, w]));
  for (const w of patch.upsert) byId.set(w.id, w);
  if (patch.order) {
    return patch.order.map((id) => {
      const w = byId.get(id);
      if (!w) throw new Error(`widget order names unknown widget ${id}`);
      return w;
    });
  }
  // Same order: replace in place, append the new ones.
  const known = new Set(current.map((w) => w.id));
  return [...current.map((w) => byId.get(w.id) as WidgetView), ...patch.upsert.filter((w) => !known.has(w.id))];
}

/** The interface's groups after `patch`, in stacking order. A group is replaced as a whole. */
export function applyHud(current: readonly HudGroup[], patch: HudPatch): HudGroup[] {
  const byName = new Map(current.map((g) => [g.name, g]));
  for (const name of patch.removed) byName.delete(name);
  for (const g of patch.groups) byName.set(g.name, g);
  for (const m of patch.moved) {
    const g = byName.get(m.name);
    if (g) byName.set(m.name, { ...g, offset: m.offset });
  }
  return [...byName.values()].sort((a, b) => a.layer - b.layer);
}

/** A patch that carries every topic, as the one the game sends after `Resync`. */
export function isSnapshot(patch: ViewPatch): boolean {
  return (
    patch.mode != null &&
    patch.screen != null &&
    patch.screen.widgets.t === 'full' &&
    patch.cursor !== undefined &&
    patch.loading !== undefined
  );
}
