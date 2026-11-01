import type { Part } from '../protocol/Part';
import type { WidgetView } from '../protocol/WidgetView';

/** The widgets a sheet shows, in reading order (top to bottom, then left to right). Pictures
 * (unless Rust names them as controls), outlines, areas and scroll bars are the classic skin's decoration; a button with neither label
 * nor key has nothing to tell the player; a widget that is not shown (hidden, or outside
 * its clip window) is left out, as the classic skin does not draw it; the open drop-down is the combo box's own business. */
export function sheetItems(widgets: readonly WidgetView[]): WidgetView[] {
  return widgets
    .filter((w) => {
      if (w.id.endsWith('/dropdown')) return false;
      // Hidden or clipped away (the main menu keeps its submenus' entries off to the side).
      if (!w.shown) return false;
      switch (w.kind) {
        case 'button':
          // A picture button (OK, Cancel) has no text, only the name Rust gave it.
          return Boolean(w.label || w.key);
        case 'text':
        case 'strip':
          return Boolean(w.label);
        case 'picture':
          // A named picture is a control drawn as a picture (the random map's colour and team fields).
          return Boolean(w.key);
        case 'list':
        case 'combo':
        case 'slider':
        case 'input':
        case 'viewer':
          return w.value != null;
        default:
          return false;
      }
    })
    .sort((a, b) => a.rect[1] - b.rect[1] || a.rect[0] - b.rect[0]);
}

/** What a sheet shows for one control: the control, the static text that names it (left of it or above it in
 * the classic layout, or the column header of its table), and the static text drawn over it (the mark of a
 * picture field: the team's "X"). A lone static text is a cell of its own. */
export interface Cell {
  widget: WidgetView;
  caption?: WidgetView;
  mark?: WidgetView;
}

/** A sheet block: one cell, a card (a row of a table with its column headers as captions), or the heading of
 * the group of controls that follows it. */
export type Block = { t: 'cell'; cell: Cell } | { t: 'card'; cells: Cell[] } | { t: 'section'; heading: WidgetView };

type Rect = WidgetView['rect'];

const left = (r: Rect) => r[0];
const right = (r: Rect) => r[0] + r[2];
const top = (r: Rect) => r[1];
const bottom = (r: Rect) => r[1] + r[3];
/** How far the intervals [a0, a1] and [b0, b1] overlap (negative: the gap between them). */
const overlap = (a0: number, a1: number, b0: number, b1: number) => Math.min(a1, b1) - Math.max(a0, b0);
/** The two rects share a row: they overlap vertically by more than a third of the lower one (a label's
 * rect and the thin bar of a slider beside it only just meet). */
const sameRow = (a: Rect, b: Rect) => overlap(top(a), bottom(a), top(b), bottom(b)) > Math.min(a[3], b[3]) / 3;

const isStatic = (w: WidgetView) => w.kind === 'text';
/** A control that names itself: a button or check with its text. */
const labelled = (w: WidgetView) => (w.kind === 'button' || w.kind === 'strip') && Boolean(w.label);
/** A control that is only a picture, so that a static text lying on it is its visual. */
const bare = (w: WidgetView) => (w.kind === 'button' || w.kind === 'picture') && !w.label;

/** The controls in rows, each row left to right: controls whose centre lies within the vertical extent of
 * the row they join (the rects of a classic layout carry no row structure of their own). */
function rowsOf(controls: readonly WidgetView[]): WidgetView[][] {
  const sorted = [...controls].sort((a, b) => top(a.rect) + a.rect[3] / 2 - (top(b.rect) + b.rect[3] / 2));
  const rows: { y1: number; items: WidgetView[] }[] = [];
  for (const c of sorted) {
    const row = rows[rows.length - 1];
    if (row && top(c.rect) + c.rect[3] / 2 < row.y1) {
      row.items.push(c);
      row.y1 = Math.max(row.y1, bottom(c.rect));
    } else {
      rows.push({ y1: bottom(c.rect), items: [c] });
    }
  }
  return rows.map((r) => r.items.sort((a, b) => left(a.rect) - left(b.rect)));
}

/** A row of a table: the controls of a row that stand together (gaps no wider than three of their heights;
 * the controls of other things lying in the same pixel row, like a settings column, make other segments). */
function segments(row: readonly WidgetView[]): WidgetView[][] {
  const out: WidgetView[][] = [];
  for (const c of row) {
    const last = out[out.length - 1];
    const prev = last?.[last.length - 1];
    if (last && prev && left(c.rect) - right(prev.rect) <= 3 * Math.max(c.rect[3], prev.rect[3])) last.push(c);
    else out.push([c]);
  }
  return out;
}

const segmentRect = (s: readonly WidgetView[]): Rect => {
  const x0 = Math.min(...s.map((c) => left(c.rect)));
  const y0 = Math.min(...s.map((c) => top(c.rect)));
  return [x0, y0, Math.max(...s.map((c) => right(c.rect))) - x0, Math.max(...s.map((c) => bottom(c.rect))) - y0];
};

/** The headers of a table whose first row is `first`: the statics in one row directly above it, as many as it
 * has controls, within its width (a little beyond: a header is not always aligned to its field). */
function headersOf(first: readonly WidgetView[], statics: readonly WidgetView[]): WidgetView[] | null {
  const r = segmentRect(first);
  const slack = r[2] / 4;
  const above = statics.filter((s) => {
    const gap = r[1] - bottom(s.rect);
    return gap >= -s.rect[3] / 2 && gap <= 2 * s.rect[3] && left(s.rect) >= r[0] - slack && right(s.rect) <= right(r) + slack;
  });
  if (above.length !== first.length) return null;
  above.sort((a, b) => left(a.rect) - left(b.rect));
  return above.every((s) => sameRow(s.rect, above[0].rect)) ? above : null;
}

/** The widgets of a block (a heading is only itself) as the area they cover. */
function extentOf(parts: readonly WidgetView[]): Rect {
  const x0 = Math.min(...parts.map((w) => left(w.rect)));
  const y0 = Math.min(...parts.map((w) => top(w.rect)));
  return [x0, y0, Math.max(...parts.map((w) => right(w.rect))) - x0, Math.max(...parts.map((w) => bottom(w.rect))) - y0];
}

/** Whether the static text `t` heads a group: a static text sits below it, close and indented (the group's
 * captions lie inside the heading's column, the heading at its edge). */
function heads(t: WidgetView, below: readonly WidgetView[]): boolean {
  const h = t.rect[3];
  return below.some((s) => {
    const gap = top(s.rect) - bottom(t.rect);
    const indent = left(s.rect) - left(t.rect);
    return s.id !== t.id && gap >= -h / 2 && gap <= 1.5 * h && indent >= h / 4 && indent <= 3 * h;
  });
}

/** The sheet's blocks: the widgets of `sheetItems` grouped by the classic layout's geometry, one column of
 * them per column region of the classic layout (left to right), each in reading order (top to bottom).
 *
 * - A static text lying on a control that is only a picture is that control's mark.
 * - Several rows of controls with the same number of controls, aligned under a row of as many static texts, are
 *   a table: one card per row, each control captioned by the header of its column.
 * - A static text with another static text close below it and indented (the group's captions) is the heading of
 *   the group of controls that follows it in its column.
 * - A static text immediately left of a control (in its row) or directly above it is its caption.
 * - Everything else is a cell of its own. */
export function sheetBlocks(widgets: readonly WidgetView[]): Block[] {
  const items = sheetItems(widgets);
  const cells = new Map<string, Cell>(items.map((w) => [w.id, { widget: w }]));
  const cell = (w: WidgetView) => cells.get(w.id)!;
  const controls = items.filter((w) => !isStatic(w));
  const free = new Set(items.filter(isStatic).map((w) => w.id));
  const unclaimed = () => items.filter((w) => free.has(w.id));
  const claim = (s: WidgetView) => free.delete(s.id);
  const inCard = new Set<string>();
  // Each table's cards in a row, in the place of the table's first row.
  const cards: { cells: Cell[]; at: Rect; seq: number }[] = [];

  // Marks.
  for (const s of unclaimed()) {
    const cx = left(s.rect) + s.rect[2] / 2;
    const cy = top(s.rect) + s.rect[3] / 2;
    const on = controls.find((c) => bare(c) && cx >= left(c.rect) && cx <= right(c.rect) && cy >= top(c.rect) && cy <= bottom(c.rect));
    if (on && !cell(on).mark) {
      cell(on).mark = s;
      claim(s);
    }
  }

  // Tables.
  const rows = rowsOf(controls).flatMap(segments).filter((s) => s.length >= 2);
  rows.sort((a, b) => top(segmentRect(a)) - top(segmentRect(b)));
  const used = new Set<WidgetView[]>();
  for (const first of rows) {
    if (used.has(first)) continue;
    const stack = [first];
    for (const next of rows) {
      const last = segmentRect(stack[stack.length - 1]);
      const at = segmentRect(next);
      const h = Math.max(last[3], at[3]);
      if (
        !used.has(next) &&
        next !== first &&
        next.length === first.length &&
        top(at) >= bottom(last) - h / 2 &&
        top(at) - bottom(last) <= 2 * h &&
        Math.abs(left(at) - left(segmentRect(first))) <= 3 * h
      ) {
        stack.push(next);
      }
    }
    const headers = stack.length >= 2 ? headersOf(first, unclaimed()) : null;
    if (!headers) continue;
    headers.forEach(claim);
    for (const row of stack) {
      used.add(row);
      row.forEach((c, i) => (cell(c).caption = headers[i]));
      row.forEach((c) => inCard.add(c.id));
      cards.push({ cells: row.map(cell), at: segmentRect(first), seq: cards.length });
    }
  }

  // Headings.
  const sections = new Set<string>();
  for (const t of unclaimed()) {
    if (heads(t, unclaimed())) {
      sections.add(t.id);
      claim(t);
    }
  }

  // Captions: left of the control in its row, else directly above it.
  for (const c of controls) {
    if (labelled(c) || inCard.has(c.id)) continue;
    const row = unclaimed().filter((s) => sameRow(s.rect, c.rect) && left(c.rect) - right(s.rect) >= -s.rect[3] / 2 && left(c.rect) - right(s.rect) <= 8 * s.rect[3]);
    const above = unclaimed().filter((s) => {
      const gap = top(c.rect) - bottom(s.rect);
      return gap >= -s.rect[3] / 2 && gap <= s.rect[3] && overlap(left(s.rect), right(s.rect), left(c.rect), right(c.rect)) > 0;
    });
    const by = (a: WidgetView, b: WidgetView) => right(b.rect) - right(a.rect) || bottom(b.rect) - bottom(a.rect);
    const caption = row.sort(by)[0] ?? above.sort(by)[0];
    if (caption) {
      cell(c).caption = caption;
      claim(caption);
    }
  }

  const placed = [
    ...cards.map((c) => ({ block: { t: 'card', cells: c.cells } as Block, at: c.at, area: c.at, seq: c.seq })),
    ...items
      .filter((w) => !inCard.has(w.id) && (!isStatic(w) || free.has(w.id) || sections.has(w.id)))
      .map((w) => {
        if (sections.has(w.id)) return { block: { t: 'section', heading: w } as Block, at: w.rect, area: w.rect, seq: 0 };
        const c = cell(w);
        const parts = [c.caption, w, c.mark].filter((x): x is WidgetView => !!x);
        const at = parts.filter((x) => x !== c.mark).sort((p, q) => top(p.rect) - top(q.rect) || left(p.rect) - left(q.rect))[0].rect;
        return { block: { t: 'cell', cell: c } as Block, at, area: extentOf(parts), seq: 0 };
      }),
  ];
  // The column regions: blocks whose areas share horizontal extent, from the left (the classic layout's
  // two-column screens read the whole left column, then the right one).
  const columns: { x0: number; x1: number; blocks: typeof placed }[] = [];
  for (const p of [...placed].sort((a, b) => left(a.area) - left(b.area))) {
    const column = columns.find((c) => overlap(c.x0, c.x1, left(p.area), right(p.area)) > 0);
    if (column) {
      column.blocks.push(p);
      column.x0 = Math.min(column.x0, left(p.area));
      column.x1 = Math.max(column.x1, right(p.area));
    } else {
      columns.push({ x0: left(p.area), x1: right(p.area), blocks: [p] });
    }
  }
  columns.sort((a, b) => a.x0 - b.x0);
  return columns.flatMap((c) =>
    c.blocks.sort((a, b) => top(a.at) - top(b.at) || left(a.at) - left(b.at) || a.seq - b.seq).map((p) => p.block),
  );
}
