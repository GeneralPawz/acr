<script lang="ts">
  import { layout, type Font } from './fonts';
  import { ui } from './context';

  interface Props {
    font: string;
    set: number;
    text: string;
    x: number;
    y: number;
    /** `x` is the centre of the string instead of its start. */
    center?: boolean;
  }
  let { font, set, text, x, y, center = false }: Props = $props();

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

  // The glyphs of the string, one image each (later ones over earlier ones, as ShowString
  // draws them). A missing glyph is logged and leaves the string out.
  const placed = $derived.by(() => {
    if (!loaded) return null;
    try {
      return layout(loaded.metrics, text);
    } catch (e) {
      console.error(e);
      return null;
    }
  });
  const left = $derived(center && placed ? x - Math.floor(placed.width / 2) : x);
</script>

{#if loaded && placed}
  <div class="text" style:left="{left}px" style:top="{y}px">
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
  </div>
{/if}

<style>
  .text {
    position: absolute;
  }
  .glyph {
    position: absolute;
    background-repeat: no-repeat;
    image-rendering: var(--acr-image-rendering, pixelated);
  }
</style>
