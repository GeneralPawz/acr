<script lang="ts">
  import { onMount } from 'svelte';
  import GameViewport from './GameViewport.svelte';
  import Stage from './Stage.svelte';
  import { ui } from './context';

  const { model } = ui();
  let dpr = $state(window.devicePixelRatio || 1);

  // The DOM shows a menu screen only when the game says it leaves the screen to the UI; the
  // loading screen stays while the game loads.
  const shown = $derived(
    (model.mode === 'menus' || model.mode === 'loading') && model.screen !== null && !model.screen.engine_drawn,
  );

  onMount(() => {
    const update = () => {
      dpr = window.devicePixelRatio || 1;
    };
    window.addEventListener('resize', update);
    return () => window.removeEventListener('resize', update);
  });
</script>

<div class="acr-ui" data-acr-mode={model.mode}>
  {#if shown}
    <Stage {dpr} />
  {/if}
  <GameViewport {dpr} />
</div>

<style>
  .acr-ui {
    position: fixed;
    inset: 0;
    overflow: hidden;
    pointer-events: none;
  }
</style>
