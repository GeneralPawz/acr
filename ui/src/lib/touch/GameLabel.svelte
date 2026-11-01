<script lang="ts">
  import { ui } from '../context';
  import { layout, type Font } from '../fonts';
  import { fitScale } from './sheet';

  interface Props {
    font: string;
    set: number;
    text: string;
    /** CSS scale of the game font's pixels (the glyphs are bitmaps of the original). */
    scale?: number;
    /** Takes the width the parent leaves and shrinks the text to it (never overflows), see `fitScale`. */
    fit?: boolean;
    /** The tallest the text may be, in CSS pixels. */
    maxHeight?: number;
  }
  let { font, set, text, scale: max = 1, fit = false, maxHeight = Infinity }: Props = $props();

  const { fonts } = ui();
  let loaded = $state.raw<Font | null>(null);

  $effect(() => {
    let live = true;
    fonts.get(set, font).then(
      (f) => {
        if (live) loaded = f;
      },
      (e) => console.error(`font ${font}:`, e),
    );
    return () => {
      live = false;
    };
  });

  const placed = $derived.by(() => {
    if (!loaded) return null;
    try {
      return layout(loaded.metrics, text);
    } catch (e) {
      console.error(e);
      return null;
    }
  });

  let avail = $state(0);
  const scale = $derived(placed && loaded ? fitScale(placed.width, loaded.metrics.height, max, fit ? avail : 0, maxHeight) : max);
</script>

<!-- A string of an original font in the flow of the page: a box of its natural size times
     `scale`, the glyph images inside. The text is also the label for assistive technology. -->
<span class="slot" class:fit bind:clientWidth={avail}>
<span
  class="label"
  class:smooth={scale < 1}
  role="img"
  aria-label={text}
  style:width="{placed ? placed.width * scale : 0}px"
  style:height="{loaded ? loaded.metrics.height * scale : 0}px"
>
  {#if loaded && placed}
    <span class="glyphs" style:transform="scale({scale})">
      {#each placed.glyphs as g, i (i)}
        <span
          class="glyph"
          style:left="{g.x}px"
          style:top="{g.y}px"
          style:width="{g.w}px"
          style:height="{g.h}px"
          style:background-image="url({loaded.atlas})"
          style:background-position="-{g.sx}px -{g.sy}px"
        ></span>
      {/each}
    </span>
  {/if}
</span>
</span>

<style>
  /* A fitted label owns the free width of its row and centres the text in it; `avail` is that width. */
  .slot {
    display: contents;
  }
  .slot.fit {
    display: flex;
    flex: 1 1 0;
    min-width: 0;
    justify-content: center;
  }
  .label {
    display: inline-block;
    position: relative;
    flex: none;
    vertical-align: middle;
  }
  .glyphs {
    position: absolute;
    left: 0;
    top: 0;
    transform-origin: 0 0;
  }
  .glyph {
    position: absolute;
    background-repeat: no-repeat;
    image-rendering: pixelated;
  }
  .smooth .glyph {
    image-rendering: auto;
  }
</style>
