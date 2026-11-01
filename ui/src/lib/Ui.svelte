<script lang="ts">
  import { onMount } from 'svelte';
  import Hud from './Hud.svelte';
  import GameViewport from './GameViewport.svelte';
  import Stage from './Stage.svelte';
  import { ui } from './context';
  import { pixelRatio } from './pixels';
  import { resolveSkin } from './skin';
  import TouchHud from './touch/TouchHud.svelte';
  import TouchMenu from './touch/TouchMenu.svelte';

  const { bridge, model } = ui();
  let dpr = $state(pixelRatio());
  // The page, for the skin: CSS pixels and whether the primary pointer is a finger.
  const coarse = window.matchMedia?.('(pointer: coarse)');
  let page = $state({ width: window.innerWidth, height: window.innerHeight, coarse: coarse?.matches ?? false });
  const skin = $derived(resolveSkin(model.skinSetting, page));
  // The 9-slice source of the original's frame, from the player's files.
  // and the paper and painting of the touch skin's sheets, as CSS `url()`s.
  let art = $state<Record<string, string>>({});

  // The game learns the skin once it has answered (it knows the player's `ui_skin`), and
  // whenever the page changes it: the touch skin takes the interface's pixels away from the game.
  $effect(() => {
    if (model.protocol !== null) bridge.send({ t: 'skin', skin });
  });

  const ART = {
    '--acr-nine': 'nine/box.png',
    '--acr-parchment': 'nine/parchment.png',
    '--acr-backdrop': 'bmp/INTERF2%5CBACKGROUND_MAIN_MENU.BMP',
  };
  $effect(() => {
    if (skin !== 'touch') return;
    let live = true;
    for (const [name, path] of Object.entries(ART)) {
      bridge.assetUrl(path).then(
        (url) => {
          if (live) art[name] = `url(${url})`;
        },
        (e) => console.error(`${path}:`, e),
      );
    }
    return () => {
      live = false;
    };
  });

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
      dpr = pixelRatio();
      page = { width: window.innerWidth, height: window.innerHeight, coarse: coarse?.matches ?? false };
    };
    window.addEventListener('resize', update);
    coarse?.addEventListener?.('change', update);
    return () => {
      window.removeEventListener('resize', update);
      coarse?.removeEventListener?.('change', update);
    };
  });
</script>

<div class="acr-ui" data-acr-mode={model.mode} data-acr-skin={skin} style={Object.entries(art).map(([k, v]) => `${k}:${v}`).join(";")}>
  {#if skin === 'touch'}
    <GameViewport {dpr} touch />
    {#if model.mode === 'game'}
      <TouchHud />
    {/if}
    {#if shown}
      <TouchMenu />
    {/if}
  {:else}
    {#if model.mode === 'game' && model.hud.length > 0}
      <Hud {dpr} />
    {/if}
    {#if shown}
      <Stage {dpr} />
    {/if}
    <GameViewport {dpr} />
  {/if}
</div>

<style>
  .acr-ui {
    position: fixed;
    inset: 0;
    overflow: hidden;
    pointer-events: none;
  }
</style>
