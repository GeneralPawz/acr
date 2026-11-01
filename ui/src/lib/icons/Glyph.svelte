<script lang="ts">
  import { glyph } from './glyphs';

  interface Props {
    /** The glyph's name (`IconView.glyph`, or a bar button's). */
    name: string;
    /** A plaque of dark leather with a gold rule round the glyph (the command grid); off: the bare line art. */
    plaque?: boolean;
  }
  let { name, plaque = false }: Props = $props();

  // The markup is a constant of ./glyphs, never input.
  const art = $derived(glyph(name) ?? '');
</script>

<svg class="glyph" viewBox="0 0 48 48" aria-hidden="true" data-acr-glyph={name}>
  {#if plaque}
    <rect class="leather" x="1" y="1" width="46" height="46" rx="6" />
    <rect class="rule" x="3.5" y="3.5" width="41" height="41" rx="4" />
  {/if}
  <g class="art" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round">
    <!-- eslint-disable-next-line svelte/no-at-html-tags -->
    {@html art}
  </g>
</svg>

<style>
  .glyph {
    display: block;
    width: 100%;
    height: 100%;
    color: var(--touch-gold, #e2c27a);
  }
  .leather {
    fill: var(--touch-leather, #2b1b0e);
    stroke: var(--touch-gold-dim, #b08d4c);
    stroke-width: 1.6;
  }
  .rule {
    fill: none;
    stroke: var(--touch-line, rgb(226 194 122 / 0.28));
    stroke-width: 0.9;
  }
</style>
