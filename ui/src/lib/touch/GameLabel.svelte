<script lang="ts">
  import { ui } from '../context';
  import { layout, type Font } from '../fonts';

  interface Props {
    font: string;
    set: number;
    text: string;
    /** CSS scale of the game font's pixels (the glyphs are bitmaps of the original). */
    scale?: number;
  }
  let { font, set, text, scale = 1 }: Props = $props();

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
</script>

<!-- A string of an original font in the flow of the page: a box of its natural size times
     `scale`, the glyph images inside. The text is also the label for assistive technology. -->
<span
  class="label"
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

<style>
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
</style>
