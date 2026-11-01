import type { Part } from '../protocol/Part';
import type { WidgetView } from '../protocol/WidgetView';

/** A font of the original and its glyph set, as the engine drew a widget's text. */
export interface FontRef {
  font: string;
  set: number;
}

/** Used when a widget drew no text of its own (a picture button). */
export const DEFAULT_FONT: FontRef = { font: 'SmallWhiteFont', set: 1 };

/** The font the widget's own text was drawn with. */
export function fontOf(widget: WidgetView): FontRef {
  const text = widget.parts.find((p): p is Extract<Part, { t: 'text' }> => p.t === 'text');
  return text ? { font: text.font, set: text.set } : DEFAULT_FONT;
}

/** The widgets a sheet shows, in reading order (top to bottom, then left to right). Pictures,
 * outlines, areas and scroll bars are the classic skin's decoration; a button with neither label
 * nor key has nothing to tell the player; the open drop-down is the combo box's own business. */
export function sheetItems(widgets: readonly WidgetView[]): WidgetView[] {
  return widgets
    .filter((w) => {
      if (w.id.endsWith('/dropdown')) return false;
      switch (w.kind) {
        case 'button':
          // A picture button (OK, Cancel) has no text, only the name Rust gave it.
          return Boolean(w.label || w.key);
        case 'text':
        case 'strip':
          return Boolean(w.label);
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

/** The words of a text, for a viewer that wraps like a page. */
export function words(text: string): string[] {
  return text.split(/\s+/).filter((w) => w.length > 0);
}
