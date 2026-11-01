<script lang="ts">
  import { onMount } from 'svelte';
  import { ui } from '../context';
  import Part from '../Part.svelte';
  import type { IconView } from '../protocol/IconView';
  import type { Part as PartView } from '../protocol/Part';
  import { gridColumns, portrait as isPortrait, sizeClass } from '../skin';
  import GameLabel from './GameLabel.svelte';
  import { intent, leftClick } from './intents';
  import Sprite from './Sprite.svelte';
  import { words } from './sheet';

  const { bridge, model } = ui();

  // CSS pixels of the viewport (transient: the page's size).
  let size = $state({ width: window.innerWidth, height: window.innerHeight });
  const klass = $derived(sizeClass(size));
  const portrait = $derived(isPortrait(size));
  const columns = $derived(gridColumns(klass));
  // Side of an icon's cell; the art is 40 pixels.
  const cell = $derived(klass === 'compact' ? 52 : klass === 'medium' ? 60 : 64);
  const iconScale = $derived((cell - 4) / 40);
  const mapWidth = $derived(klass === 'compact' ? 112 : klass === 'medium' ? 140 : 168);

  const touch = $derived(model.touch);
  const selection = $derived(touch?.icons.filter((i) => i.role === 'select') ?? []);
  const commands = $derived(touch?.icons.filter((i) => i.role !== 'select') ?? []);

  // The font the interface text is in, with the set its own parts used.
  const font = $derived.by(() => {
    for (const r of touch?.resources ?? []) {
      const text = r.parts.find((p): p is Extract<PartView, { t: 'text' }> => p.t === 'text');
      if (text) return { font: text.font, set: text.set };
    }
    return { font: 'YellowFont', set: 1 };
  });

  // The minimap is the classic skin's three groups, cut to the map image.
  const mapGroups = $derived(['minimap', 'minimap_frame', 'minimap_view'].map((n) => model.hud.find((g) => g.name === n)));
  const mapImage = $derived(mapGroups[0]?.parts.find((p): p is Extract<PartView, { t: 'sprite' }> => p.t === 'sprite') ?? null);
  const mapScale = $derived(mapImage ? mapWidth / mapImage.w : 1);
  let mapOpen = $state(true);

  /** A press that lasts this long shows the hint instead of tapping. */
  const LONG_PRESS_MS = 450;
  let hint = $state<string | null>(null);
  let hintTimer: ReturnType<typeof setTimeout> | undefined;
  let pressTimer: ReturnType<typeof setTimeout> | undefined;
  let long = false;

  function press(onLong: () => void) {
    long = false;
    clearTimeout(pressTimer);
    pressTimer = setTimeout(() => {
      long = true;
      onLong();
    }, LONG_PRESS_MS);
  }

  function release() {
    clearTimeout(pressTimer);
  }

  function showHint(text: string) {
    if (!text) return;
    hint = text;
    clearTimeout(hintTimer);
    hintTimer = setTimeout(() => (hint = null), 2500);
  }

  function tapIcon(icon: IconView) {
    if (long) {
      long = false;
      return;
    }
    if (icon.clickable) intent(bridge, icon.id, leftClick);
  }

  function tapBar(id: string) {
    intent(bridge, id, leftClick);
  }

  function uv(e: PointerEvent): [number, number] {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    return [Math.min(Math.max((e.clientX - r.left) / r.width, 0), 1), Math.min(Math.max((e.clientY - r.top) / r.height, 0), 1)];
  }

  let mapAt: [number, number] = [0, 0];
  function mapDown(e: PointerEvent) {
    mapAt = uv(e);
    press(() => intent(bridge, 'minimap', { t: 'minimap_at', uv: mapAt, button: 'right' }));
  }
  function mapUp(e: PointerEvent) {
    release();
    if (long) {
      long = false;
      return;
    }
    intent(bridge, 'minimap', { t: 'minimap_at', uv: uv(e), button: 'left' });
  }

  onMount(() => {
    const update = () => (size = { width: window.innerWidth, height: window.innerHeight });
    window.addEventListener('resize', update);
    return () => {
      window.removeEventListener('resize', update);
      clearTimeout(hintTimer);
      clearTimeout(pressTimer);
    };
  });
</script>

{#snippet iconButton(icon: IconView)}
  <button
    class="icon"
    class:selected={icon.selected}
    aria-disabled={!icon.clickable}
    aria-label={icon.hint || icon.id}
    data-acr-id={icon.id}
    data-acr-role={icon.role}
    onpointerdown={() => press(() => showHint(icon.hint))}
    onpointerup={release}
    onpointercancel={release}
    onpointerleave={release}
    oncontextmenu={(e) => e.preventDefault()}
    onclick={() => tapIcon(icon)}
  >
    <Sprite parts={icon.parts} size={icon.size} scale={iconScale} />
  </button>
{/snippet}

{#if touch}
  <div
    class="hud"
    class:portrait
    class:landscape={!portrait}
    data-acr-touch="hud"
    data-acr-size={klass}
    style:--cell="{cell}px"
    style:--cols={columns}
  >
    <div class="upper">
    <header class="top">
      <ul class="resources">
        {#each touch.resources as r (r.key)}
          <li data-acr-resource={r.key}><Sprite parts={r.parts} size={r.size} /></li>
        {/each}
      </ul>
      <div class="buttons">
      <button class="bar" aria-label="pause" data-acr-id="bar/pause" onclick={() => tapBar('bar/pause')}>
        <GameLabel {...font} text={touch.paused ? '>' : 'II'} scale={2} />
      </button>
      <button class="bar" aria-label="menu" data-acr-id="bar/menu" onclick={() => tapBar('bar/menu')}>
        <GameLabel {...font} text="M" scale={2} />
      </button>
      </div>
    </header>

    {#if touch.minimap && mapImage}
      <div class="minimap" data-acr-touch="minimap">
        <button class="fold" aria-label="minimap" aria-expanded={mapOpen} onclick={() => (mapOpen = !mapOpen)}>
          <GameLabel {...font} text={mapOpen ? '-' : '+'} scale={2} />
        </button>
        {#if mapOpen}
          <div
            class="map"
            data-acr-id="minimap"
            role="button"
            tabindex="0"
            aria-label="minimap"
            style:width="{mapImage.w * mapScale}px"
            style:height="{mapImage.h * mapScale}px"
            onpointerdown={mapDown}
            onpointerup={mapUp}
            onpointercancel={release}
            oncontextmenu={(e) => e.preventDefault()}
          >
            <div class="canvas" style:left="{-mapImage.x * mapScale}px" style:top="{-mapImage.y * mapScale}px" style:transform="scale({mapScale})">
              {#each mapGroups as group, g (g)}
                {#if group}
                  {#each group.parts as part, i (i)}
                    <Part {part} />
                  {/each}
                {/if}
              {/each}
            </div>
          </div>
        {/if}
      </div>
    {/if}

    </div>

    {#if hint}
      <div class="hint" role="status" data-acr-touch="hint">
        {#each words(hint) as word, i (i)}
          <GameLabel {...font} text={word} scale={1.5} />
        {/each}
      </div>
    {/if}

    <aside class="commands" data-acr-touch="commands">
      {#if selection.length > 0 || (touch.army.length > 0)}
        <div class="summary" data-acr-touch="summary">
          {#each selection as icon (icon.id)}
            {@render iconButton(icon)}
          {/each}
          {#each touch.army as icon (icon.id)}
            {@render iconButton(icon)}
          {/each}
        </div>
      {/if}
      <div class="grid" data-acr-touch="grid">
        {#each commands as icon (icon.id)}
          {@render iconButton(icon)}
        {/each}
      </div>
    </aside>
  </div>
{/if}

<style>
  .hud {
    --pad: 8px;
    position: fixed;
    inset: 0;
    box-sizing: border-box;
    height: 100dvh;
    padding: env(safe-area-inset-top) env(safe-area-inset-right) env(safe-area-inset-bottom) env(safe-area-inset-left);
    pointer-events: none;
    color: #f2e3bd;
  }
  .upper {
    position: absolute;
    top: env(safe-area-inset-top);
    left: env(safe-area-inset-left);
    right: env(safe-area-inset-right);
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 4px;
    pointer-events: none;
  }
  .landscape .upper {
    right: calc(env(safe-area-inset-right) + var(--cols) * (var(--cell) + 4px) + 2 * var(--pad));
  }
  .top {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
    box-sizing: border-box;
    padding: 4px var(--pad);
    background: rgb(20 14 8 / 0.78);
    pointer-events: auto;
  }
  .resources {
    display: grid;
    grid-template-columns: repeat(6, auto);
    flex: 1 1 200px;
    gap: 2px 14px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .portrait[data-acr-size='compact'] .resources {
    grid-template-columns: repeat(3, auto);
  }
  .buttons {
    display: flex;
    flex: none;
    gap: 6px;
  }
  .bar,
  .fold {
    display: flex;
    align-items: center;
    justify-content: center;
    box-sizing: border-box;
    min-width: 48px;
    min-height: 48px;
    padding: 0 10px;
    color: inherit;
    background: #3a2a18 var(--acr-nine-fill, none);
    border: 12px solid transparent;
    border-image: var(--acr-nine, none) 64 / 12px round;
    cursor: pointer;
    -webkit-tap-highlight-color: transparent;
  }
  .minimap {
    align-self: flex-end;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 4px;
    margin-right: var(--pad);
    pointer-events: none;
  }
  .fold,
  .map {
    pointer-events: auto;
  }
  .fold {
    min-height: 44px;
    min-width: 44px;
  }
  .map {
    position: relative;
    overflow: hidden;
    border: 3px solid #d9b46a;
    background: #000;
    touch-action: none;
  }
  .canvas {
    position: absolute;
    transform-origin: 0 0;
  }
  .hint {
    position: absolute;
    top: calc(env(safe-area-inset-top) + 64px);
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    flex-wrap: wrap;
    gap: 4px 10px;
    box-sizing: border-box;
    max-width: calc(100% - 24px);
    padding: 8px 12px;
    background: rgb(20 14 8 / 0.92);
    border: 2px solid #d9b46a;
    pointer-events: none;
  }
  .commands {
    position: absolute;
    display: flex;
    flex-direction: column;
    gap: 6px;
    box-sizing: border-box;
    padding: 6px var(--pad);
    background: rgb(20 14 8 / 0.8);
    pointer-events: auto;
  }
  .portrait .commands {
    left: env(safe-area-inset-left);
    right: env(safe-area-inset-right);
    bottom: env(safe-area-inset-bottom);
  }
  .landscape .commands {
    right: env(safe-area-inset-right);
    top: env(safe-area-inset-top);
    bottom: env(safe-area-inset-bottom);
    justify-content: flex-end;
    width: calc(var(--cols) * (var(--cell) + 4px) + 2 * var(--pad));
    overflow-y: auto;
  }
  .summary,
  .grid {
    display: grid;
    grid-template-columns: repeat(var(--cols), var(--cell));
    justify-content: center;
    gap: 4px;
  }
  .grid {
    max-height: calc(3 * (var(--cell) + 4px));
    overflow-y: auto;
  }
  .summary {
    padding-bottom: 6px;
    border-bottom: 1px solid #5b4126;
  }
  .icon {
    display: block;
    box-sizing: border-box;
    width: var(--cell);
    height: var(--cell);
    padding: 0;
    background: none;
    border: 2px solid transparent;
    cursor: pointer;
    touch-action: manipulation;
    -webkit-tap-highlight-color: transparent;
    -webkit-touch-callout: none;
  }
  .icon.selected {
    border-color: #ffc840;
  }
  .icon[aria-disabled='true'] {
    cursor: default;
  }
</style>
