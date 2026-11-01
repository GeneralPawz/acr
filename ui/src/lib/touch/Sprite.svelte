<script lang="ts">
  import Part from '../Part.svelte';
  import type { Part as PartView } from '../protocol/Part';

  interface Props {
    parts: PartView[];
    /** The box the draws lie in, in the art's own pixels. */
    size: [number, number];
    /** Where the box's top-left corner lies in the draws' coordinates. */
    origin?: [number, number];
    /** CSS scale of the box. */
    scale?: number;
  }
  let { parts, size, origin = [0, 0], scale = 1 }: Props = $props();
</script>

<!-- The draws of a piece of the original interface, laid out by Rust in a box of their own and
     scaled as a whole. -->
<span class="sprite" aria-hidden="true" style:width="{size[0] * scale}px" style:height="{size[1] * scale}px">
  <span class="box" style:width="{size[0]}px" style:height="{size[1]}px" style:transform="scale({scale})">
    <span class="shift" style:left="{-origin[0]}px" style:top="{-origin[1]}px">
      {#each parts as part, i (i)}
        <Part {part} />
      {/each}
    </span>
  </span>
</span>

<style>
  .sprite {
    display: block;
    position: relative;
    flex: none;
    overflow: hidden;
  }
  .shift {
    position: absolute;
  }
  .box {
    position: absolute;
    left: 0;
    top: 0;
    transform-origin: 0 0;
  }
</style>
