<script lang="ts" module>
  let serial = 0;
</script>

<script lang="ts">
  import { glyph } from './glyphs';

  interface Props {
    /** The glyph's name (`IconView.glyph`, or a bar button's). */
    name: string;
    /** A plaque of chiselled dark leather with a gold rule round the glyph (the command grid); off: the bare drawing. */
    plaque?: boolean;
  }
  let { name, plaque = false }: Props = $props();

  // The markup is a constant of ./glyphs, never input.
  const art = $derived(glyph(name) ?? '');
  // Gradients are looked up by id in the whole page, so every glyph names its own (a folded panel hides its first).
  const id = `acr-g${++serial}`;
</script>

<svg
  class="glyph"
  viewBox="0 0 48 48"
  aria-hidden="true"
  data-acr-glyph={name}
  style:--gold="url(#{id}-gold)"
  style:--iron="url(#{id}-iron)"
  style:--wood="url(#{id}-wood)"
  style:--red="url(#{id}-red)"
  style:--skin="url(#{id}-skin)"
  style:--ink="url(#{id}-ink)"
>
  <defs>
    <!-- Light from the upper left, as in the original icons: every ramp runs top left to bottom right across the grid. -->
    <linearGradient id="{id}-gold" gradientUnits="userSpaceOnUse" x1="8" y1="6" x2="40" y2="42">
      <stop offset="0" class="gold-hi" />
      <stop offset=".45" class="gold" />
      <stop offset="1" class="gold-lo" />
    </linearGradient>
    <linearGradient id="{id}-iron" gradientUnits="userSpaceOnUse" x1="8" y1="6" x2="40" y2="42">
      <stop offset="0" stop-color="#e6e9ea" />
      <stop offset=".5" stop-color="#9aa2a8" />
      <stop offset="1" stop-color="#4d555c" />
    </linearGradient>
    <linearGradient id="{id}-wood" gradientUnits="userSpaceOnUse" x1="8" y1="6" x2="40" y2="42">
      <stop offset="0" stop-color="#c79a62" />
      <stop offset=".5" stop-color="#8a5c32" />
      <stop offset="1" stop-color="#4f3118" />
    </linearGradient>
    <linearGradient id="{id}-red" gradientUnits="userSpaceOnUse" x1="8" y1="6" x2="40" y2="42">
      <stop offset="0" stop-color="#e0705c" />
      <stop offset=".5" stop-color="#a8301f" />
      <stop offset="1" stop-color="#5c150e" />
    </linearGradient>
    <linearGradient id="{id}-skin" gradientUnits="userSpaceOnUse" x1="8" y1="6" x2="40" y2="42">
      <stop offset="0" stop-color="#ecc190" />
      <stop offset="1" stop-color="#a06b44" />
    </linearGradient>
    <linearGradient id="{id}-ink" gradientUnits="userSpaceOnUse" x1="8" y1="6" x2="40" y2="42">
      <stop offset="0" stop-color="#3a2a1a" />
      <stop offset="1" stop-color="#150c05" />
    </linearGradient>
    <linearGradient id="{id}-plaque" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" class="leather-hi" />
      <stop offset=".55" class="leather-mid" />
      <stop offset="1" class="leather-lo" />
    </linearGradient>
    <!-- The stone's grain: a few flecks, light and dark, on a tile that repeats. -->
    <pattern id="{id}-grain" width="9" height="9" patternUnits="userSpaceOnUse">
      <rect x="1" y="2" width="1.4" height="1" fill="#fff" fill-opacity=".07" />
      <rect x="6" y="1" width="1" height="1.4" fill="#000" fill-opacity=".16" />
      <rect x="4" y="5" width="1.6" height="1" fill="#000" fill-opacity=".12" />
      <rect x="7.5" y="6.5" width="1" height="1" fill="#fff" fill-opacity=".06" />
      <rect x="1.5" y="7" width="1" height="1.2" fill="#000" fill-opacity=".14" />
    </pattern>
  </defs>
  {#if plaque}
    <rect class="shadow" x="1.6" y="2" width="46" height="46" rx="6" />
    <rect class="leather" x="1" y="1" width="46" height="46" rx="6" fill="url(#{id}-plaque)" />
    <rect x="1" y="1" width="46" height="46" rx="6" fill="url(#{id}-grain)" />
    <!-- The chiselled edge: lit on the upper left, in shade on the lower right. -->
    <path class="bevel-lit" d="M2.2 40V8a5.8 5.8 0 0 1 5.8-5.8h32" />
    <path class="bevel-dark" d="M46 8v32a6 6 0 0 1-6 6H8" />
    <rect class="rule" x="3.5" y="3.5" width="41" height="41" rx="4" />
  {/if}
  <g class="art" fill="none" stroke-linecap="round" stroke-linejoin="round">
    <g class="sh">
      <!-- eslint-disable-next-line svelte/no-at-html-tags -->
      {@html art}
    </g>
    <g class="ol">
      <!-- eslint-disable-next-line svelte/no-at-html-tags -->
      {@html art}
    </g>
    <g class="bd">
      <!-- eslint-disable-next-line svelte/no-at-html-tags -->
      {@html art}
    </g>
    <g class="hi">
      <!-- eslint-disable-next-line svelte/no-at-html-tags -->
      {@html art}
    </g>
    <g class="lo">
      <!-- eslint-disable-next-line svelte/no-at-html-tags -->
      {@html art}
    </g>
  </g>
</svg>

<style>
  .glyph {
    display: block;
    width: 100%;
    height: 100%;
    color: var(--touch-gold, #e2c27a);
    overflow: visible;
  }
  /* The theme's colours are the ramps' ends. */
  .gold-hi {
    stop-color: color-mix(in srgb, var(--touch-gold, #e2c27a) 55%, #fff);
  }
  .gold {
    stop-color: var(--touch-gold, #e2c27a);
  }
  .gold-lo {
    stop-color: color-mix(in srgb, var(--touch-gold-dim, #b08d4c) 62%, #000);
  }
  .leather-hi {
    stop-color: color-mix(in srgb, var(--touch-leather, #2b1b0e) 78%, #b9a58a);
  }
  .leather-mid {
    stop-color: var(--touch-leather, #2b1b0e);
  }
  .leather-lo {
    stop-color: color-mix(in srgb, var(--touch-leather, #2b1b0e) 70%, #000);
  }
  .shadow {
    fill: #000;
    fill-opacity: 0.45;
  }
  .leather {
    stroke: #100a04;
    stroke-width: 1.4;
  }
  .bevel-lit {
    fill: none;
    stroke: color-mix(in srgb, var(--touch-gold, #e2c27a) 55%, #fff);
    stroke-opacity: 0.5;
    stroke-width: 1.1;
    stroke-linecap: round;
  }
  .bevel-dark {
    fill: none;
    stroke: #000;
    stroke-opacity: 0.55;
    stroke-width: 1.3;
    stroke-linecap: round;
  }
  .rule {
    fill: none;
    stroke: var(--touch-line, rgb(226 194 122 / 0.28));
    stroke-width: 0.9;
  }

  /* The drawing, five times over the same markup: shadow, outline, body, highlight and shade. */
  .sh {
    transform: translate(0.9px, 1.4px);
    stroke-width: 3.6;
  }
  .ol {
    stroke-width: 4.4;
  }
  .bd {
    stroke-width: 2.4;
    --m: var(--gold);
  }
  .hi,
  .lo {
    stroke-width: 0.8;
  }
  .hi {
    transform: translate(-0.55px, -0.55px);
  }
  .lo {
    transform: translate(0.6px, 0.6px);
  }
  .glyph :global(.sh *:not([stroke='none'])),
  .glyph :global(.ol *:not([stroke='none'])) {
    stroke: #120a04;
  }
  .glyph :global(.sh [fill='currentColor']),
  .glyph :global(.ol [fill='currentColor']) {
    fill: #120a04;
  }
  .glyph :global(.sh) {
    opacity: 0.5;
  }
  .glyph :global(.sh *:not([stroke='none'])) {
    stroke: #000;
  }
  .glyph :global(.sh [fill='currentColor']) {
    fill: #000;
  }
  .glyph :global(.bd *:not([stroke='none'])) {
    stroke: var(--m);
  }
  .glyph :global(.bd [fill='currentColor']) {
    fill: var(--m);
  }
  .glyph :global(.bd .iron) {
    --m: var(--iron);
  }
  .glyph :global(.bd .wood) {
    --m: var(--wood);
  }
  .glyph :global(.bd .red) {
    --m: var(--red);
  }
  .glyph :global(.bd .skin) {
    --m: var(--skin);
  }
  .glyph :global(.bd .ink) {
    --m: var(--ink);
  }
  .glyph :global(.hi *:not([stroke='none'])) {
    stroke: #fff;
    stroke-opacity: 0.6;
    fill: none;
  }
  .glyph :global(.lo *:not([stroke='none'])) {
    stroke: #000;
    stroke-opacity: 0.3;
    fill: none;
  }
  .glyph :global(.hi [fill='currentColor'][stroke='none']),
  .glyph :global(.lo [fill='currentColor'][stroke='none']) {
    display: none;
  }
</style>
