import { describe, expect, it } from 'vitest';
import type { WidgetView } from '../protocol/WidgetView';
import { DEFAULT_FONT, fitScale, fontOf, sheetBlocks, sheetItems, words } from './sheet';

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

  it('keeps a named picture as a control and drops the others', () => {
    const items = sheetItems([widget('S/0', 'picture', [0, 0, 9, 9]), widget('S/1', 'picture', [0, 20, 9, 9], { key: 'color_0' })]);
    expect(items.map((w) => w.id)).toEqual(['S/1']);
  });
});

describe('sheet blocks', () => {
  const text = (id: string, rect: [number, number, number, number], label: string) => widget(id, 'text', rect, { label });
  const combo = (id: string, rect: [number, number, number, number]) =>
    widget(id, 'combo', rect, { value: { t: 'combo', options: ['a'], selected: 0, open: false } });
  const ids = (blocks: ReturnType<typeof sheetBlocks>) =>
    blocks.map((b) => (b.t === 'cell' ? b.cell.widget.id + (b.cell.caption ? '<' + b.cell.caption.id : '') + (b.cell.mark ? '+' + b.cell.mark.id : '') : b.t === 'card' ? b.cells.map((c) => c.widget.id + '<' + c.caption?.id) : '#' + b.heading.id));

  it('captions a control with the text left of it and the text above it', () => {
    const blocks = sheetBlocks([
      text('S/0', [0, 100, 50, 6], 'Landscape'),
      combo('S/1', [56, 99, 80, 9]),
      text('S/2', [0, 200, 40, 6], 'Name'),
      widget('S/3', 'input', [0, 207, 90, 9], { value: { t: 'input', text: 'x', capacity: 8 } }),
      text('S/4', [0, 20, 40, 8], 'Title'),
      widget('S/5', 'button', [0, 300, 40, 9], { label: 'OK' }),
    ]);
    expect(ids(blocks)).toEqual(['S/4', 'S/1<S/0', 'S/3<S/2', 'S/5']);
  });

  it('does not caption a labelled button with the text beside it', () => {
    const blocks = sheetBlocks([text('S/0', [0, 100, 50, 6], 'Note'), widget('S/1', 'button', [56, 101, 80, 9], { label: 'Go' })]);
    expect(ids(blocks)).toEqual(['S/0', 'S/1']);
  });

  it('shows the text lying on a picture field as that field', () => {
    const blocks = sheetBlocks([
      widget('S/0', 'picture', [150, 100, 12, 8], { key: 'team_0' }),
      text('S/1', [154, 99, 5, 6], 'X'),
      widget('S/2', 'picture', [150, 200, 12, 8]),
      text('S/3', [152, 200, 5, 6], 'Y'),
    ]);
    // The unkeyed picture is decoration and the text on it stays as it is.
    expect(ids(blocks)).toEqual(['S/0+S/1', 'S/3']);
  });

  // The random map's player table and its settings column beside it, as the classic layout puts them.
  const randomMap = (): WidgetView[] => {
    const out: WidgetView[] = [
      text('R/1', [105, 316, 18, 5], 'Spieler'),
      text('R/2', [37, 330, 18, 5], 'Spieler'),
      text('R/3', [87, 330, 23, 5], 'Nationen'),
      text('R/4', [140, 330, 15, 5], 'Farbe'),
      text('R/5', [172, 330, 14, 5], 'Team'),
      text('R/6', [236, 331, 33, 5], 'Eigene Karte'),
      text('R/7', [284, 331, 32, 5], 'Zufallskarte'),
    ];
    for (let i = 0; i < 3; i++) {
      const y = 336 + 10 * i;
      out.push(
        i === 0 ? widget('R/n0', 'input', [35, y, 46, 9], { value: { t: 'input', text: 'EINSTEIN', capacity: 20 } }) : combo(`R/p${i}`, [35, y + 1, 49, 9]),
        combo(`R/c${i}`, [85, y + 1, 48, 9]),
        widget(`R/f${i}`, 'picture', [135, y + 1, 13, 8], { key: `color_${i}` }),
        widget(`R/t${i}`, 'picture', [152, y + 3, 12, 7], { key: `team_${i}` }),
        text(`R/x${i}`, [155, y + 1, 5, 6], 'X'),
        text(`R/l${i}`, [231, y + 10, 46, 6], `Option ${i}`),
        combo(`R/o${i}`, [282, y + 10, 79, 9]),
      );
    }
    return out;
  };

  it('makes a card of each row of a table, captioned by its column headers', () => {
    const blocks = sheetBlocks(randomMap());
    const cards = blocks.filter((b) => b.t === 'card');
    expect(cards).toHaveLength(3);
    expect(ids(cards)).toEqual([
      ['R/n0<R/2', 'R/c0<R/3', 'R/f0<R/4', 'R/t0<R/5'],
      ['R/p1<R/2', 'R/c1<R/3', 'R/f1<R/4', 'R/t1<R/5'],
      ['R/p2<R/2', 'R/c2<R/3', 'R/f2<R/4', 'R/t2<R/5'],
    ]);
    expect(cards.flatMap((b) => (b.t === 'card' ? b.cells : [])).map((c) => c.mark?.id).filter(Boolean)).toEqual(['R/x0', 'R/x1', 'R/x2']);
  });

  it('keeps the headers out of the sheet, and the settings column captioned beside the table', () => {
    const blocks = sheetBlocks(randomMap());
    const rest = ids(blocks.filter((b) => b.t === 'cell'));
    expect(rest).toEqual(['R/1', 'R/6', 'R/7', 'R/o0<R/l0', 'R/o1<R/l1', 'R/o2<R/l2']);
    // Reading order by column: the table's window title and the whole table, then the tabs and the settings.
    expect(blocks.map((b) => b.t)).toEqual(['cell', 'card', 'card', 'card', 'cell', 'cell', 'cell', 'cell', 'cell']);
  });

  it('captions a slider with the label whose box only just meets its thin bar', () => {
    const blocks = sheetBlocks([
      text('S/0', [172, 414, 49, 6], 'Volume'),
      widget('S/1', 'slider', [236, 418, 111, 4.6], { value: { t: 'range', value: 1, max: 9 } }),
    ]);
    expect(ids(blocks)).toEqual(['S/1<S/0']);
  });

  it('does not take a lone row of fields for a table: each is captioned on its own', () => {
    const blocks = sheetBlocks([
      text('S/0', [0, 0, 10, 5], 'A'),
      text('S/1', [30, 0, 10, 5], 'B'),
      combo('S/2', [0, 10, 25, 9]),
      combo('S/3', [30, 10, 25, 9]),
    ]);
    expect(ids(blocks)).toEqual(['S/2<S/0', 'S/3<S/1']);
  });

  // The options sheet as the classic layout has it (the driver's tree): headings at the left edge of the
  // left column, their captions indented under them, the controls in the right column; one control has its
  // heading in its own row.
  const options = (): WidgetView[] => {
    const slider = (id: string, y: number) => widget(id, 'slider', [236, y, 111, 5], { value: { t: 'range', value: 1, max: 9 } });
    return [
      text('O/3', [169, 373, 45, 6], 'Grafikoptionen'),
      text('O/4', [169, 394, 44, 6], 'Soundoptionen'),
      text('O/5', [169, 435, 39, 6], 'Spieloptionen'),
      text('O/6', [172, 384, 48, 6], 'Grafikauflösung'),
      text('O/7', [172, 404, 18, 6], 'Musik'),
      text('O/8', [172, 414, 49, 6], 'Soundlautstärke'),
      text('O/9', [172, 425, 49, 6], 'Musiklautstärke'),
      text('O/10', [172, 445, 33, 6], 'Spielmodus'),
      combo('O/20', [236, 383, 112, 9]),
      combo('O/22', [236, 404, 112, 9]),
      slider('O/16', 418),
      slider('O/17', 428),
      combo('O/23', [236, 435, 112, 9]),
      combo('O/21', [236, 446, 112, 9]),
      widget('O/14', 'button', [180, 495, 69, 8], { label: 'OK' }),
    ];
  };

  it('puts a heading above its group, and a control in the heading row keeps no caption', () => {
    const blocks = sheetBlocks(options());
    expect(blocks.map((b) => (b.t === 'section' ? '#' + b.heading.id : b.t === 'cell' ? b.cell.widget.id + (b.cell.caption ? '<' + b.cell.caption.id : '') : '?'))).toEqual([
      '#O/3',
      'O/20<O/6',
      '#O/4',
      'O/22<O/7',
      'O/16<O/8',
      'O/17<O/9',
      '#O/5',
      'O/23',
      'O/21<O/10',
      'O/14',
    ]);
  });

  it('reads the left column of a two-column screen before the right one', () => {
    const blocks = sheetBlocks([
      combo('S/r0', [200, 10, 80, 9]),
      combo('S/l1', [0, 30, 80, 9]),
      combo('S/l0', [0, 10, 80, 9]),
      combo('S/r1', [200, 30, 80, 9]),
    ]);
    expect(ids(blocks)).toEqual(['S/l0', 'S/l1', 'S/r0', 'S/r1']);
  });
});
