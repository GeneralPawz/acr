<script lang="ts">
  import GameText from './GameText.svelte';
  import { ui } from './context';
  import type { Part } from './protocol/Part';

  let { part }: { part: Part } = $props();

  const { bridge } = ui();
  let src = $state<string | undefined>();

  $effect(() => {
    if (part.t !== 'sprite') return;
    let live = true;
    bridge.assetUrl(part.url).then(
      (url) => {
        if (live) src = url;
      },
      (e) => console.error(`asset ${part.t === 'sprite' ? part.url : ''}:`, e),
    );
    return () => {
      live = false;
    };
  });

  // A clip is a window onto the canvas: the part sits in a box that cuts it off, shifted back
  // so its own coordinates stay canvas coordinates.
  const clip = $derived(part.clip);
</script>

<div
  class="clip"
  style:left="{clip ? clip[0] : 0}px"
  style:top="{clip ? clip[1] : 0}px"
  style:width={clip ? `${clip[2]}px` : undefined}
  style:height={clip ? `${clip[3]}px` : undefined}
  style:overflow={clip ? 'hidden' : undefined}
>
  <div class="origin" style:left="{clip ? -clip[0] : 0}px" style:top="{clip ? -clip[1] : 0}px">
    {#if part.t === 'sprite'}
      {#if src}
        <img
          alt=""
          draggable="false"
          {src}
          style:left="{part.x}px"
          style:top="{part.y}px"
          style:width="{part.w}px"
          style:height="{part.h}px"
          style:opacity={part.alpha}
        />
      {/if}
    {:else if part.t === 'text'}
      <GameText font={part.font} set={part.set} text={part.text} x={part.x} y={part.y} />
    {:else}
      <div
        class="fill"
        style:left="{part.rect[0]}px"
        style:top="{part.rect[1]}px"
        style:width="{part.rect[2]}px"
        style:height="{part.rect[3]}px"
        style:background-color="rgba({part.rgba[0]}, {part.rgba[1]}, {part.rgba[2]}, {part.rgba[3] / 255})"
      ></div>
    {/if}
  </div>
</div>

<style>
  .clip,
  .origin {
    position: absolute;
  }
  img {
    position: absolute;
    image-rendering: pixelated;
    user-select: none;
  }
  .fill {
    position: absolute;
  }
</style>
