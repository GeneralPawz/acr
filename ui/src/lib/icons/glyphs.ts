/**
 * The touch skin's own icons for the HUD's commands (docs/webui.md "Touch skin"): original line art,
 * one hand on one 48 x 48 grid. Every glyph keeps to the same rules so they read as one set:
 *
 * - strokes 2.4 wide with round caps and joins, in the current colour, nothing thinner;
 * - the drawing lies in the square 8..40 (the plaque and the badge corner take the rest);
 * - solid shapes (`fill="currentColor"`) only for small things: heads, rank dots, pips, arrow tips;
 * - a command that switches something on and off is the same drawing twice, the off one with the
 *   `no` bar through it or, for work that stops and resumes, a pause or play badge in the corner.
 *
 * The values are markup for the inside of an `<svg viewBox="0 0 48 48">` whose `<g>` sets the stroke;
 * they are constants of this file, never user input. The key is the glyph's name as the engine sends it
 * (`IconView.glyph`; `hud/touch_skin.rs` `glyph`) or a bar button's.
 */

/** A rank dot. */
const dot = (x: number, y: number, r = 2.6) => `<circle cx="${x}" cy="${y}" r="${r}" fill="currentColor" stroke="none"/>`;
const dots = (points: [number, number][], r = 2.6) => points.map(([x, y]) => dot(x, y, r)).join('');

/** A man: head and shoulders, centred on `x`. */
const man = (x: number, y = 0) =>
  `<circle cx="${x}" cy="${y + 15}" r="3.4"/><path d="M${x - 6.5} ${y + 33}v-5a6.5 6.5 0 0 1 13 0v5z"/>`;

/** A musket from the butt at the lower left to the muzzle at the upper right. */
const musket =
  '<g transform="rotate(-40 24 24)"><path d="M15 22h25v4H15zM15 22l-10 8v3h7l5-5M20 26q0 4 4 3"/></g>';

/** A sword upright in the middle of the grid. */
const sword = '<path d="M24 8v22M21.5 10.5L24 7l2.5 3.5M17 30h14M24 30v6"/><circle cx="24" cy="38.5" r="2"/>';

/** Two crossed swords. */
const crossed =
  '<path d="M11 11l20 20M26 36l10-10M33 33l5 5M37 11L17 31M22 36L12 26M15 33l-5 5"/>' +
  '<circle cx="39.5" cy="39.5" r="1.8"/><circle cx="8.5" cy="39.5" r="1.8"/>';

/** The corner badge for work that stopped (two bars) or runs again (a triangle). */
const badge = (play: boolean) =>
  `<circle cx="35" cy="35" r="8.5" fill="#2b1c10"/>` +
  (play ? '<path d="M32.5 30.5l7 4.5-7 4.5z" fill="currentColor"/>' : '<path d="M32.5 30.5v9M37.5 30.5v9"/>');

/** `art` smaller in the upper left, for the badged glyphs. */
const small = (art: string) => `<g transform="translate(-1 -1) scale(.86)">${art}</g>`;

/** The bar of a thing that is off. */
const no = '<path d="M11 11l26 26"/>';

const hammer = '<path d="M17 12l9-1 6 6-1 9-5-5-3-2zM22 21L10 35"/>';
const fish = '<path d="M8 24q9-11 20 0-11 11-20 0zM28 24l9-7v14z"/>' + dot(15, 22, 1.4);
const wheat =
  '<path d="M24 40V14M24 21q-7-1-8-8 7 0 8 8zM24 21q7-1 8-8-7 0-8 8zM24 29q-7-1-8-8M24 29q7-1 8-8"/>' +
  '<path d="M24 13l-2-5M24 13l2-5"/>';
const cannon = '<path d="M7 21l22-3v11L7 28zM29 17v13"/><circle cx="18" cy="35" r="5"/>';

/** Rows of a formation, as in the original: the soldiers stand as dots. */
const formation = {
  line: dots([[10, 19], [17, 19], [24, 19], [31, 19], [38, 19], [10, 29], [17, 29], [24, 29], [31, 29], [38, 29]]),
  column: dots([[19, 11], [29, 11], [19, 19], [29, 19], [19, 27], [29, 27], [19, 35], [29, 35]]),
  square: dots([
    [12, 12], [20, 12], [28, 12], [36, 12], [12, 20], [36, 20], [12, 28], [36, 28], [12, 36], [20, 36], [28, 36], [36, 36],
  ]),
  wedge: dots([[24, 10], [19, 18], [29, 18], [14, 26], [34, 26], [9, 34], [39, 34]]),
  crescent: dots([[10, 14], [12, 24], [18, 31], [24, 33], [30, 31], [36, 24], [38, 14]]),
  rider: (x: number) => `<path d="M${x} 11v14"/>` + dot(x, 29, 3.2),
};

const trio = (n: number) =>
  [[24], [19, 29], [14, 24, 34]][n - 1]
    .map((x) => `<path d="M${x} 16v16M${x - 2.6} 16h5.2M${x - 2.6} 32h5.2"/>`)
    .join('');

/** A sign in the middle of the upper grid and `pips` rank dots under it: tens, hundreds, thousands... */
const amount = (sign: '+' | '-', pips: number) =>
  `<path d="M14 19h20${sign === '+' ? 'M24 9v20' : ''}"/>` +
  dots(Array.from({ length: pips }, (_, i) => [24 + (i - (pips - 1) / 2) * 7, 36] as [number, number]), 2.4);

export const GLYPHS: Record<string, string> = {
  // Stance and fire.
  'stand-ground': '<path d="M13 10h22v13c0 8-6 12-11 15-5-3-11-7-11-15z"/><path d="M24 14v20M17 21h14"/>',
  advance: '<path d="M16 28l8-8 8 8M16 18l8-8 8 8M12 38h24"/>',
  'attack-move': crossed,
  patrol: '<path d="M11 21a14 14 0 0 1 25-6M37 8v8h-8M37 27a14 14 0 0 1-25 6M11 40v-8h8"/>',
  'hold-fire': `<g transform="translate(7.2 7.2) scale(.7)">${musket}</g>` + '<circle cx="24" cy="24" r="17"/><path d="M12 12l24 24"/>',
  'open-fire': musket + '<path d="M39 4v4M46 11h-4M44 6l-3 3"/>',
  'friendly-fire': man(13, 4) + man(35, 4) + '<path d="M13 15q11-12 22 0M30.5 12.5L35 15l.5-5.3"/>',
  'friendly-safe': man(13, 4) + man(35, 4) + '<path d="M13 15q11-12 22 0M21 6l6 6M27 6l-6 6"/>',
  'melee-only': sword,
  'any-attack':
    '<path d="M11 11l20 20M26 36l10-10M33 33l5 5"/><circle cx="39.5" cy="39.5" r="1.8"/>' +
    '<path d="M37 11L14 34M14 34l-5 5M9 39l-1-4M12 32l4 4"/>',

  // Guns and mortars.
  'attack-ground': '<ellipse cx="24" cy="36" rx="14" ry="5.5"/><path d="M24 19v10M19.5 25l4.5 4.5 4.5-4.5"/><circle cx="24" cy="11" r="3.5" fill="currentColor"/>',
  'mortar-zone': '<circle cx="24" cy="24" r="15" stroke-dasharray="4 4.2"/><path d="M24 15v18M15 24h18"/>' + dot(24, 24, 2),
  'mortar-clear': '<circle cx="24" cy="24" r="15" stroke-dasharray="4 4.2"/><path d="M17 17l14 14M31 17L17 31"/>',
  'mortar-fire': '<circle cx="21" cy="29" r="9"/><path d="M27.5 22.5l5-5"/><path d="M34 6v8M30 10h8M31 7l6 6M37 7l-6 6"/>',
  'mortar-cease': '<circle cx="21" cy="29" r="9"/><path d="M27.5 22.5l5-5"/><path d="M38 38L10 10"/>',
  'aim-guide': '<path d="M8 36Q20 4 36 28" stroke-dasharray="1 4.6"/><circle cx="37" cy="33" r="5"/><path d="M37 25v4M37 37v4M29 33h4M41 33h4"/>',
  'canister-fire': cannon + dots([[36, 17], [39, 24], [36, 31], [43, 20], [43, 28]], 1.8),
  'canister-load': cannon + '<path d="M15 6h10v8H15zM15 6a5 2 0 0 1 10 0"/>',

  // Work that stops and resumes: the thing and a corner badge.
  'production-stop': small(hammer) + badge(false),
  'production-resume': small(hammer) + badge(true),
  'fishing-stop': small(fish) + badge(false),
  'fishing-resume': small(fish) + badge(true),
  'sowing-stop': small(wheat) + badge(false),
  'sowing-resume': small(wheat) + badge(true),

  // Formations and groups.
  disband: '<path d="M10 17v-7h7M31 10h7v7M38 31v7h-7M17 38h-7v-7"/>' + dots([[20, 20], [28, 20], [20, 28], [28, 28]], 2),
  group: '<rect x="8" y="12" width="32" height="24" rx="4"/><rect x="13" y="19" width="8" height="10"/><rect x="27" y="19" width="8" height="10"/>',
  ungroup: '<rect x="8" y="12" width="32" height="24" rx="4" stroke-dasharray="5 4"/><rect x="13" y="19" width="8" height="10"/><rect x="27" y="19" width="8" height="10"/>',
  'form-smaller': dots([[10, 17], [20, 17], [30, 17], [10, 27], [20, 27], [30, 27]], 2.2) + '<path d="M33 37h8"/>',
  'form-larger': dots([[10, 17], [20, 17], [30, 17], [10, 27], [20, 27], [30, 27]], 2.2) + '<path d="M33 37h8M37 33v8"/>',
  'form-fill': dots([[12, 12], [24, 12], [36, 12], [12, 24], [24, 24], [36, 24], [12, 36], [24, 36]]) + '<circle cx="36" cy="36" r="2.6" stroke-dasharray="2 2"/>',
  'formation-line': formation.line,
  'formation-column': formation.column,
  'formation-square': formation.square,
  'formation-wedge': formation.wedge,
  'formation-crescent': formation.crescent,
  'formation-cavalry-line': [10, 20, 30, 40].map((x) => formation.rider(x - 1)).join('') + '<path d="M8 36h32"/>',
  'army-list': dots([[11, 14], [11, 24], [11, 34]], 2.2) + '<path d="M19 14h20M19 24h20M19 34h20"/>',
  rally: '<path d="M14 40V8M14 10h21l-5 7 5 7H14M9 40h10"/>',
  'idle-peasant': man(18, -2) + '<path d="M34 9v28M30 9h8M30 38h8l-1 4h-6z"/>',
  'idle-mine': '<path d="M12 40L32 14M21 9Q37 3 41 23"/>',

  // The building's pages: I, II, III.
  'page-1': trio(1),
  'page-2': trio(2),
  'page-3': trio(3),

  // The market.
  'market-trade': '<path d="M9 18h28M31 12l6 6-6 6M39 30H11M17 24l-6 6 6 6"/>',
  'market-clear': '<circle cx="24" cy="24" r="14"/><path d="M18 18l12 12M30 18L18 30"/>',
  'amount-minus-1': amount('-', 1),
  'amount-minus-2': amount('-', 2),
  'amount-minus-3': amount('-', 3),
  'amount-minus-4': amount('-', 4),
  'amount-plus-1': amount('+', 1),
  'amount-plus-2': amount('+', 2),
  'amount-plus-3': amount('+', 3),
  'amount-plus-4': amount('+', 4),

  // The top bar.
  pause: '<path d="M17 11v26M31 11v26"/>',
  play: '<path d="M16 10l20 14-20 14z" fill="currentColor"/>',
  menu: '<path d="M10 14h28M10 24h28M10 34h28"/>',
  'map-open': '<path d="M9 14l10-4 10 4 10-4v24l-10 4-10-4-10 4zM19 10v24M29 14v24"/>',
  'resources-closed': '<circle cx="24" cy="15" r="9"/><circle cx="24" cy="15" r="4.2"/><path d="M14 34l10 8 10-8"/>',
  'resources-open': '<circle cx="24" cy="33" r="9"/><circle cx="24" cy="33" r="4.2"/><path d="M14 14l10-8 10 8"/>',
  'map-closed': '<path d="M9 14l10-4 10 4 10-4v24l-10 4-10-4-10 4zM19 10v24M29 14v24M13 40L35 8"/>',
};

/** The glyph's markup; `undefined` for a name this set does not draw (the page then shows the original's picture). */
export function glyph(name: string | null | undefined): string | undefined {
  return name ? GLYPHS[name] : undefined;
}
