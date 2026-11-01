<script lang="ts">
  import Part from './Part.svelte';
  import { ui } from './context';

  const { model } = ui();
  let { dpr }: { dpr: number } = $props();

  // The in-game screen as `Canvas::screen` lays it out, in CSS pixels. The interface is drawn on
  // the transparent page over the game's canvas, under any dialog (`Stage`); a group with a
  // `hud_scale` is stretched about its anchor.
  const game = $derived(model.stage?.game ?? null);
  // The engine stretches the game screen with a linear filter; only at a whole scale does that
  // leave the pixels as they are, which `pixelated` then matches.
  const smooth = $derived(game !== null && !Number.isInteger(game.scale / dpr));
</script>

{#if game}
  <div
    class="hud"
    data-acr-hud
    style:left="{game.origin[0] / dpr}px"
    style:top="{game.origin[1] / dpr}px"
    style:width="{game.size[0]}px"
    style:height="{game.size[1]}px"
    style:transform="scale({game.scale / dpr})"
    style:--acr-image-rendering={smooth ? 'auto' : 'pixelated'}
  >
    {#each model.hud as group (group.name)}
      <div
        class="group"
        data-acr-hud-group={group.name}
        data-acr-hud-scale={group.scale}
        data-acr-hud-anchor={group.anchor.join(',')}
        style:transform={group.scale === 1 ? undefined : `scale(${group.scale})`}
        style:transform-origin="{group.anchor[0]}px {group.anchor[1]}px"
      >
        {#each group.parts as part, i (i)}
          <Part {part} />
        {/each}
      </div>
    {/each}
  </div>
{/if}

<style>
  .hud {
    position: absolute;
    overflow: hidden;
    transform-origin: 0 0;
    pointer-events: none;
  }
  .group {
    position: absolute;
    left: 0;
    top: 0;
  }
</style>
