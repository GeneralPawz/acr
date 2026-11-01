<script lang="ts">
  import Hint from './Hint.svelte';
  import Widget from './Widget.svelte';
  import { ui } from './context';

  const { model } = ui();
  let { dpr }: { dpr: number } = $props();

  // The 1024x768 canvas as `Canvas::fit` lays it out, in CSS pixels: the same maths at any
  // scale, so the window's pixels and the DOM's agree.
  const menu = $derived(model.menu);
  // The UI draws this screen (not the engine): on Android its stage is the page's own, the rest of
  // the window belongs to the game (`android.ts`).
  const drawn = $derived(model.widgets.length > 0);
</script>

{#if menu}
  <div
    class="stage"
    data-acr-screen={model.screen?.name}
    data-acr-hit={drawn ? '' : undefined}
    style:left="{menu.origin[0] / dpr}px"
    style:top="{menu.origin[1] / dpr}px"
    style:width="{menu.size[0]}px"
    style:height="{menu.size[1]}px"
    style:transform="scale({menu.scale / dpr})"
  >
    {#each model.widgets as widget (widget.id)}
      <Widget {widget} />
    {/each}
    {#if model.screen?.hint}
      <Hint hint={model.screen.hint} />
    {/if}
  </div>
{/if}

<style>
  .stage {
    position: absolute;
    overflow: hidden;
    transform-origin: 0 0;
    pointer-events: none;
  }
</style>
