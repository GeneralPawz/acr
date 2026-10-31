import type { Bridge } from './bridge';

/** `[x, y, w, h, left, top, advance]` of a glyph: its place in the atlas, where its top-left
 * pixel lies from the pen and how far the pen moves. */
export type GlyphMetrics = [number, number, number, number, number, number, number];

export interface FontMetrics {
  height: number;
  glyphs: Record<string, GlyphMetrics>;
}

export interface Font {
  metrics: FontMetrics;
  /** URL of the atlas PNG. */
  atlas: string;
}

/** One glyph image of a laid-out string, relative to the string's `ShowString` position. */
export interface PlacedGlyph {
  x: number;
  y: number;
  w: number;
  h: number;
  /** Position in the atlas. */
  sx: number;
  sy: number;
}

/** Lays `text` out like `ShowString`: the pen advances by each glyph's advance; glyph images
 * are stacked in order, so later ones (and their shadows) lie over earlier ones. A character
 * the atlas lacks is an error, not a gap. */
export function layout(metrics: FontMetrics, text: string): { glyphs: PlacedGlyph[]; width: number } {
  const glyphs: PlacedGlyph[] = [];
  let pen = 0;
  for (const ch of text) {
    const g = metrics.glyphs[ch];
    if (!g) throw new Error(`the font has no glyph for ${JSON.stringify(ch)}`);
    const [sx, sy, w, h, left, top, advance] = g;
    if (w > 0 && h > 0) glyphs.push({ x: pen + left, y: top, w, h, sx, sy });
    pen += advance;
  }
  return { glyphs, width: pen };
}

/** Loads fonts once from the game's atlas endpoints. */
export class FontCache {
  readonly #bridge: Bridge;
  readonly #fonts = new Map<string, Promise<Font>>();

  constructor(bridge: Bridge) {
    this.#bridge = bridge;
  }

  get(set: number, name: string): Promise<Font> {
    const key = `${set}/${name}`;
    let font = this.#fonts.get(key);
    if (!font) {
      font = this.#load(`font/${key}`);
      font.catch(() => this.#fonts.delete(key));
      this.#fonts.set(key, font);
    }
    return font;
  }

  async #load(base: string): Promise<Font> {
    const [json, atlas] = await Promise.all([
      this.#bridge.assetBytes(`${base}.json`),
      this.#bridge.assetUrl(`${base}.png`),
    ]);
    return { metrics: JSON.parse(new TextDecoder().decode(json)) as FontMetrics, atlas };
  }
}
