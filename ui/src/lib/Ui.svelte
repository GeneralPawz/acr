<script lang="ts">
  import { onMount } from 'svelte';
  import Hud from './Hud.svelte';
  import GameViewport from './GameViewport.svelte';
  import Stage from './Stage.svelte';
  import { ui } from './context';

  const { model } = ui();
  let dpr = $state(window.devicePixelRatio || 1);

  // The DOM shows a screen only when the game says it leaves the screen to the UI; the
  // loading screen stays while the game loads. In mode `game` only a dialog over the running game
  // is drawn here, on top of the canvas that goes on drawing the world and the HUD; everything
  // outside it reaches the game through the viewport, as the engine's modal dialogs do.
  const shown = $derived(
    model.screen !== null &&
      !model.screen.engine_drawn &&
      (model.mode === 'menus' || model.mode === 'loading' || (model.mode === 'game' && model.screen.in_game)),
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
  {#if model.mode === 'game' && model.hud.length > 0}
    <Hud {dpr} />
  {/if}
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
