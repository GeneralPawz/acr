/**
 * The touch skin's own icons for the HUD's commands (docs/webui.md "Touch skin"): original drawings,
 * one hand on one 48 x 48 grid, made to stand next to the game's painted icons. Every glyph keeps to
 * the same rules so they read as one set:
 *
 * - the drawing is line art and a few solid shapes; `Glyph.svelte` draws the same markup four times
 *   (drop shadow, dark outline, the body in its material, bevel highlight and shade), so the strokes
 *   come out as lit, rounded metal with light from the upper left, as in the original icons;
 * - strokes 2.4 wide with round caps and joins, nothing thinner;
 * - the drawing lies in the square 8..40 (the plaque and the badge corner take the rest);
 * - solid shapes (`fill="currentColor"`, which the body paints in its material) for heads, rank dots, pips,
 *   blades, cloth and iron heads;
 * - the material is the element's class: none is gold, `iron` (steel), `wood`, `red` (cloth), `skin`, `ink`
 *   (the dark disc of a badge); a class on a group is not inherited, put it on the element;
 * - a command that switches something on and off is the same drawing twice, the off one with the
 *   `no` bar through it or, for work that stops and resumes, a pause or play badge in the corner.
 *
 * The values are markup for the inside of an `<svg viewBox="0 0 48 48">`; they are constants of this
 * file, never user input. The key is the glyph's name as the engine sends it (`IconView.glyph`;
 * `hud/touch_skin.rs` `glyph`) or a bar button's.
 */

/** A solid shape in a material (`''` is gold). */
const solid = (d: string, m = '') => `<path${m ? ` class="${m}"` : ''} fill="currentColor" d="${d}"/>`;
/** A stroked line in a material. */
const line = (d: string, m = '') => `<path${m ? ` class="${m}"` : ''} d="${d}"/>`;

/** A rank dot. */
const dot = (x: number, y: number, r = 2.6, m = '') =>
  `<circle${m ? ` class="${m}"` : ''} cx="${x}" cy="${y}" r="${r}" fill="currentColor" stroke="none"/>`;
const dots = (points: [number, number][], r = 2.6, m = '') => points.map(([x, y]) => dot(x, y, r, m)).join('');

/** A blade from the guard `(x1, y1)` to the tip `(x2, y2)`: a tapering solid of width `w`. */
const blade = (x1: number, y1: number, x2: number, y2: number, w = 3.4) => {
  const dx = x2 - x1;
  const dy = y2 - y1;
  const len = Math.hypot(dx, dy);
  const nx = (-dy / len) * (w / 2);
  const ny = (dx / len) * (w / 2);
  const bx = x1 + dx * 0.82;
  const by = y1 + dy * 0.82;
  const p = (x: number, y: number) => `${x.toFixed(1)} ${y.toFixed(1)}`;
  return solid(`M${p(x1 + nx, y1 + ny)}L${p(bx + nx, by + ny)}L${p(x2, y2)}L${p(bx - nx, by - ny)}L${p(x1 - nx, y1 - ny)}z`, 'iron');
};

/** A man: head and shoulders, centred on `x`. */
const man = (x: number, y = 0) =>
  `<circle class="skin" cx="${x}" cy="${y + 15}" r="3.4" fill="currentColor"/>` +
  solid(`M${x - 6.5} ${y + 33}v-5a6.5 6.5 0 0 1 13 0v5z`, 'red');

/** A musket from the butt at the lower left to the muzzle at the upper right. */
const musket =
  '<g transform="rotate(-40 24 24)">' +
  solid('M15 22l-10 8v3h7l5-5z', 'wood') +
  solid('M15 22h25v4H15z', 'iron') +
  line('M20 26q0 4 4 3', 'iron') +
  '</g>';

/** A sword upright in the middle of the grid. */
const sword =
  blade(24, 30, 24, 6, 4.4) + '<path d="M17 30h14"/>' + line('M24 30v6', 'wood') + '<circle cx="24" cy="38.5" r="2" fill="currentColor"/>';

/** Two crossed swords. */
const crossed =
  blade(11, 11, 31, 31, 3) +
  blade(37, 11, 17, 31, 3) +
  '<path d="M26 36l10-10M22 36L12 26"/>' +
  line('M33 33l5 5M15 33l-5 5', 'wood') +
  dot(39.5, 39.5, 2) +
  dot(8.5, 39.5, 2);

/** The corner badge for work that stopped (two bars) or runs again (a triangle). */
const badge = (play: boolean) =>
  `<circle class="ink" cx="35" cy="35" r="8.5" fill="currentColor"/>` +
  (play ? '<path d="M32.5 30.5l7 4.5-7 4.5z" fill="currentColor"/>' : '<path d="M32.5 30.5v9M37.5 30.5v9"/>');

/** `art` smaller in the upper left, for the badged glyphs. */
const small = (art: string) => `<g transform="translate(-1 -1) scale(.86)">${art}</g>`;

/** The bar of a thing that is off. */
const no = '<path d="M11 11l26 26"/>';

const hammer = line('M22 21L10 35', 'wood') + solid('M17 12l9-1 6 6-1 9-5-5-3-2z', 'iron');
const fish = solid('M8 24q9-11 20 0-11 11-20 0zM28 24l9-7v14z', 'iron') + dot(15, 22, 1.4, 'ink');
const wheat =
  '<path d="M24 40V14M24 21q-7-1-8-8 7 0 8 8zM24 21q7-1 8-8-7 0-8 8zM24 29q-7-1-8-8M24 29q7-1 8-8"/>' +
  '<path d="M24 13l-2-5M24 13l2-5"/>';
const cannon =
  solid('M7 21l22-3v11L7 28z', 'iron') +
  '<path d="M29 17v13"/>' +
  '<circle class="wood" cx="18" cy="35" r="5" fill="currentColor"/><circle cx="18" cy="35" r="1.4" fill="currentColor" stroke="none"/>';

/** Rows of a formation, as in the original: the soldiers stand as dots. */
const formation = {
  line: dots([[10, 19], [17, 19], [24, 19], [31, 19], [38, 19], [10, 29], [17, 29], [24, 29], [31, 29], [38, 29]]),
  column: dots([[19, 11], [29, 11], [19, 19], [29, 19], [19, 27], [29, 27], [19, 35], [29, 35]]),
  square: dots([
    [12, 12], [20, 12], [28, 12], [36, 12], [12, 20], [36, 20], [12, 28], [36, 28], [12, 36], [20, 36], [28, 36], [36, 36],
  ]),
  wedge: dots([[24, 10], [19, 18], [29, 18], [14, 26], [34, 26], [9, 34], [39, 34]]),
  crescent: dots([[10, 14], [12, 24], [18, 31], [24, 33], [30, 31], [36, 24], [38, 14]]),
  rider: (x: number) => line(`M${x} 11v14`, 'wood') + dot(x, 29, 3.2),
};

/** The roman numeral I, II or III: stone-cut pillars with serifs, in the middle of the upper grid. */
const trio = (n: number) =>
  [[24], [19, 29], [14, 24, 34]][n - 1]
    .map((x) => solid(`M${x - 1.8} 17h3.6v14h-3.6zM${x - 4.4} 14.5h8.8v3.4h-8.8zM${x - 4.4} 30.1h8.8v3.4h-8.8z`))
    .join('');

/** A sign in the middle of the upper grid and `pips` rank dots under it: tens, hundreds, thousands... */
const amount = (sign: '+' | '-', pips: number) =>
  `<path d="M14 19h20${sign === '+' ? 'M24 9v20' : ''}"/>` +
  dots(Array.from({ length: pips }, (_, i) => [24 + (i - (pips - 1) / 2) * 7, 36] as [number, number]), 2.4);

const grid = dots([[10, 17], [20, 17], [30, 17], [10, 27], [20, 27], [30, 27]], 2.2);
const half = '<rect x="13" y="19" width="8" height="10" fill="currentColor" class="iron"/><rect x="27" y="19" width="8" height="10" fill="currentColor" class="iron"/>';

export const GLYPHS: Record<string, string> = {
  // Stance and fire.
  'stand-ground':
    '<path class="iron" fill="currentColor" d="M13 10h22v13c0 8-6 12-11 15-5-3-11-7-11-15z"/><path d="M24 14v20M17 21h14"/>',
  advance: '<path d="M16 28l8-8 8 8M16 18l8-8 8 8M12 38h24"/>',
  'attack-move': crossed,
  patrol: '<path d="M11 21a14 14 0 0 1 25-6M37 8v8h-8M37 27a14 14 0 0 1-25 6M11 40v-8h8"/>',
  'hold-fire': `<g transform="translate(7.2 7.2) scale(.7)">${musket}</g>` + '<circle cx="24" cy="24" r="17"/><path d="M12 12l24 24"/>',
  'open-fire': musket + '<path d="M39 4v4M46 11h-4M44 6l-3 3"/>',
  'friendly-fire': man(13, 4) + man(35, 4) + '<path d="M13 15q11-12 22 0M30.5 12.5L35 15l.5-5.3"/>',
  'friendly-safe': man(13, 4) + man(35, 4) + '<path d="M13 15q11-12 22 0M21 6l6 6M27 6l-6 6"/>',
  'melee-only': sword,
  'any-attack':
    blade(11, 11, 31, 31) +
    '<path d="M26 36l10-10"/>' + line('M33 33l5 5', 'wood') + dot(39.5, 39.5, 2) +
    blade(37, 11, 16, 32, 3) + '<path d="M9 39l-1-4M12 32l4 4"/>' + line('M14 34l-5 5', 'wood'),

  // Guns and mortars.
  'attack-ground':
    '<ellipse cx="24" cy="36" rx="14" ry="5.5"/><path d="M24 19v10M19.5 25l4.5 4.5 4.5-4.5"/>' + dot(24, 11, 3.8, 'iron'),
  'mortar-zone': '<circle cx="24" cy="24" r="15" stroke-dasharray="4 4.2"/><path d="M24 15v18M15 24h18"/>' + dot(24, 24, 2),
  'mortar-clear': '<circle cx="24" cy="24" r="15" stroke-dasharray="4 4.2"/><path d="M17 17l14 14M31 17L17 31"/>',
  'mortar-fire':
    '<circle class="iron" cx="21" cy="29" r="9" fill="currentColor"/><path d="M27.5 22.5l5-5"/><path d="M34 6v8M30 10h8M31 7l6 6M37 7l-6 6"/>',
  'mortar-cease':
    '<circle class="iron" cx="21" cy="29" r="9" fill="currentColor"/><path d="M27.5 22.5l5-5"/><path d="M38 38L10 10"/>',
  'aim-guide': '<path d="M8 36Q20 4 36 28" stroke-dasharray="1 4.6"/><circle cx="37" cy="33" r="5"/><path d="M37 25v4M37 37v4M29 33h4M41 33h4"/>',
  'canister-fire': cannon + dots([[36, 17], [39, 24], [36, 31], [43, 20], [43, 28]], 1.8, 'iron'),
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
  group: '<rect x="8" y="12" width="32" height="24" rx="4"/>' + half,
  ungroup: '<rect x="8" y="12" width="32" height="24" rx="4" stroke-dasharray="5 4"/>' + half,
  'form-smaller': grid + '<path d="M33 37h8"/>',
  'form-larger': grid + '<path d="M33 37h8M37 33v8"/>',
  'form-fill': dots([[12, 12], [24, 12], [36, 12], [12, 24], [24, 24], [36, 24], [12, 36], [24, 36]]) + '<circle cx="36" cy="36" r="2.6" stroke-dasharray="2 2"/>',
  'formation-line': formation.line,
  'formation-column': formation.column,
  'formation-square': formation.square,
  'formation-wedge': formation.wedge,
  'formation-crescent': formation.crescent,
  'formation-cavalry-line': [10, 20, 30, 40].map((x) => formation.rider(x - 1)).join('') + '<path d="M8 36h32"/>',
  'army-list': dots([[11, 14], [11, 24], [11, 34]], 2.2, 'red') + '<path d="M19 14h20M19 24h20M19 34h20"/>',
  rally: line('M14 40V8', 'wood') + solid('M14 10h21l-5 7 5 7H14z', 'red') + line('M9 40h10', 'wood') + dot(14, 7, 2),
  'idle-peasant':
    man(18, -2) + line('M34 9v28', 'wood') + '<path d="M30 9h8"/>' + solid('M30 38h8l-1 4h-6z', 'iron'),
  'idle-mine': line('M12 40L32 14', 'wood') + solid('M17 12Q37 3 42 25Q34 14 24 15z', 'iron'),

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
  'resources-closed': '<circle cx="24" cy="15" r="9" fill="currentColor"/><circle class="ink" cx="24" cy="15" r="4.2" fill="currentColor" stroke="none"/><path d="M14 34l10 8 10-8"/>',
  'resources-open': '<circle cx="24" cy="33" r="9" fill="currentColor"/><circle class="ink" cx="24" cy="33" r="4.2" fill="currentColor" stroke="none"/><path d="M14 14l10-8 10 8"/>',
  'map-closed': '<path d="M9 14l10-4 10 4 10-4v24l-10 4-10-4-10 4zM19 10v24M29 14v24M13 40L35 8"/>',
};

/** The glyph's markup; `undefined` for a name this set does not draw (the page then shows the original's picture). */
export function glyph(name: string | null | undefined): string | undefined {
  return name ? GLYPHS[name] : undefined;
}
