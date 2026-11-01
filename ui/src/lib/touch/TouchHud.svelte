<script lang="ts">
  import { onMount } from 'svelte';
  import { ui } from '../context';
  import './theme.css';
  import Part from '../Part.svelte';
  import type { IconView } from '../protocol/IconView';
  import type { ResourceView } from '../protocol/ResourceView';
  import type { Part as PartView } from '../protocol/Part';
  import { gridColumns, portrait as isPortrait, sizeClass } from '../skin';
  import Glyph from '../icons/Glyph.svelte';
  import { glyph as glyphArt } from '../icons/glyphs';
  import { ctrlClick, intent, leftClick } from './intents';
  import { STOPS, nearestStop, speedOf, ticksPerSecond } from './speed';
  import EditorDrawer from './EditorDrawer.svelte';
  import Sprite from './Sprite.svelte';
  import { EDGE_PX, classify } from './swipe';

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

  // The test editor's drawer (transient: open or closed is the page's own state); the bar's height places it.
  const editor = $derived(model.editor);
  let editorOpen = $state(false);
  let barHeight = $state(0);
  // Not an editor game any more: the drawer goes with it.
  $effect(() => {
    if (!editor) editorOpen = false;
  });

  // A finger that starts in the edge strip and travels left opens the drawer; any other touch there does nothing.
  let edge: { id: number; x: number; y: number } | null = null;
  function edgeDown(e: PointerEvent) {
    edge = { id: e.pointerId, x: e.clientX, y: e.clientY };
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
  }
  function edgeMove(e: PointerEvent) {
    if (!edge || edge.id !== e.pointerId) return;
    const got = classify(edge, { x: e.clientX, y: e.clientY }, 'open');
    if (got === 'open') editorOpen = true;
    if (got !== 'none') edge = null;
  }
  function edgeUp() {
    edge = null;
  }

  // The resource panel's fold, like the minimap's (transient: per session, the page's own state).
  let resourcesOpen = $state(true);
  const hasResources = $derived((touch?.resources.length ?? 0) > 0);

  /** The number the game drew for a resource (a lot of money has its own word). */
  function numberOf(r: ResourceView): string {
    const text = r.parts.find((p): p is Extract<PartView, { t: 'text' }> => p.t === 'text');
    return text ? text.text : String(r.value);
  }

  /** The icon of a resource: its sprites only, the number is drawn here in the skin's type. */
  function iconOf(r: ResourceView): { parts: PartView[]; size: [number, number] } {
    const parts = r.parts.filter((p) => p.t === 'sprite');
    const clip = parts.find((p) => p.clip)?.clip;
    return { parts, size: [clip ? clip[2] : r.size[0], r.size[1]] };
  }

  // The minimap is the classic skin's three groups, cut to the map image.
  const mapGroups = $derived(['minimap', 'minimap_frame', 'minimap_view'].map((n) => model.hud.find((g) => g.name === n)));
  const mapImage = $derived(mapGroups[0]?.parts.find((p): p is Extract<PartView, { t: 'sprite' }> => p.t === 'sprite') ?? null);
  const mapScale = $derived(mapImage ? mapWidth / mapImage.w : 1);
  let mapOpen = $state(true);

  // The game speed popover: one slider with a few stops (speed.ts); `dragStop` is the stop under the finger
  // until it lets go (transient), the game's value otherwise.
  let speedOpen = $state(false);
  let dragStop = $state<number | null>(null);
  const stop = $derived(dragStop ?? nearestStop(touch?.speed ?? 50));
  const rate = $derived(ticksPerSecond(STOPS[stop]));

  function pickSpeed(e: Event) {
    dragStop = Number((e.currentTarget as HTMLInputElement).value);
  }

  function setSpeed(e: Event) {
    const value = speedOf(Number((e.currentTarget as HTMLInputElement).value));
    dragStop = null;
    intent(bridge, 'bar/speed', { t: 'set_value', value });
  }

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

  /** A long press: a production icon orders endlessly (Ctrl+click on the desktop), any other shows its hint. */
  function holdIcon(icon: IconView) {
    if (icon.endless && icon.clickable) intent(bridge, icon.id, ctrlClick);
    showHint(icon.hint);
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

  /** Escape (Android's back gesture arrives as Escape) closes the level list, else the drawer, instead of
   * reaching the game, which would open its menu. */
  function escape(e: KeyboardEvent) {
    if (!editorOpen || e.key !== 'Escape') return;
    e.stopImmediatePropagation();
    e.preventDefault();
    if (e.type !== 'keydown') return;
    if (editor && editor.levels.length > 0) intent(bridge, 'editor/close', leftClick);
    else editorOpen = false;
  }

  onMount(() => {
    const update = () => (size = { width: window.innerWidth, height: window.innerHeight });
    window.addEventListener('resize', update);
    window.addEventListener('keydown', escape, true);
    window.addEventListener('keyup', escape, true);
    return () => {
      window.removeEventListener('resize', update);
      window.removeEventListener('keydown', escape, true);
      window.removeEventListener('keyup', escape, true);
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
    onpointerdown={() => press(() => holdIcon(icon))}
    onpointerup={release}
    onpointercancel={release}
    onpointerleave={release}
    oncontextmenu={(e) => e.preventDefault()}
    onclick={() => tapIcon(icon)}
  >
    {#if glyphArt(icon.glyph)}
      <!-- A command the touch skin draws itself; the engine's parts are what goes over it (outline, life bar, numbers). -->
      <span class="face" class:dim={icon.disabled}>
        <Glyph name={icon.glyph ?? ''} plaque />
        <span class="over"><Sprite parts={icon.parts} size={icon.size} scale={iconScale} /></span>
      </span>
    {:else}
      <Sprite parts={icon.parts} size={icon.size} scale={iconScale} />
    {/if}
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
    style:--bar-h="{barHeight}px"
  >
    {#if speedOpen}
      <button class="scrim" aria-label="close" data-acr-hit onclick={() => (speedOpen = false)}></button>
    {/if}
    <div class="upper">
    <header class="top" data-acr-hit bind:clientHeight={barHeight}>
      {#if hasResources}
        <button class="fold toggle" aria-label="resources" aria-expanded={resourcesOpen} data-acr-id="bar/resources" onclick={() => (resourcesOpen = !resourcesOpen)}>
          <span class="bar-glyph"><Glyph name={resourcesOpen ? 'resources-open' : 'resources-closed'} /></span>
        </button>
      {/if}
      <div class="buttons">
        {#if editor}
          <button class="bar" aria-label="editor" aria-expanded={editorOpen} data-acr-id="bar/editor" onclick={() => (editorOpen = !editorOpen)}>
            <span class="bar-glyph"><Glyph name="editor" /></span>
          </button>
        {/if}
        <button class="bar" aria-label="speed" aria-expanded={speedOpen} data-acr-id="bar/speed" onclick={() => (speedOpen = !speedOpen)}>
          <span class="bar-glyph"><Glyph name="speed" /></span>
        </button>
        <button class="bar" aria-label="pause" data-acr-id="bar/pause" onclick={() => tapBar('bar/pause')}>
          <span class="bar-glyph"><Glyph name={touch.paused ? 'play' : 'pause'} /></span>
        </button>
        <button class="bar" aria-label="menu" data-acr-id="bar/menu" onclick={() => tapBar('bar/menu')}>
          <span class="bar-glyph"><Glyph name="menu" /></span>
        </button>
        {#if touch.minimap && mapImage}
          <button class="bar" aria-label="minimap" aria-expanded={mapOpen} data-acr-id="bar/minimap" onclick={() => (mapOpen = !mapOpen)}>
            <span class="bar-glyph"><Glyph name={mapOpen ? 'map-open' : 'map-closed'} /></span>
          </button>
        {/if}
      </div>
      {#if speedOpen}
        <div class="speed" role="group" aria-label="speed" data-acr-hit data-acr-touch="speed">
          <div class="ends"><span>{touch.speed_text[0]}</span><span class="now">{rate} / s</span><span>{touch.speed_text[1]}</span></div>
          <input
            type="range"
            min="0"
            max={STOPS.length - 1}
            step="1"
            value={stop}
            aria-label="speed"
            data-acr-id="bar/speed/slider"
            oninput={pickSpeed}
            onchange={setSpeed}
          />
          <div class="ticks" aria-hidden="true">
            {#each STOPS as fps, i (fps)}
              <span class:normal={fps === 50} class:on={i === stop}></span>
            {/each}
          </div>
        </div>
      {/if}
    </header>
    {#if resourcesOpen}
      <ul class="resources" data-acr-hit data-acr-touch="resources">
        {#each touch.resources as r (r.key)}
          {@const icon = iconOf(r)}
          <li data-acr-resource={r.key}>
            <Sprite parts={icon.parts} size={icon.size} />
            <span class="num">{numberOf(r)}</span>
          </li>
        {/each}
      </ul>
    {/if}

    {#if editor?.armed && !editorOpen}
      <div class="placing" data-acr-touch="placing" data-acr-hit>
        <button class="what" data-acr-id="placing/open" onclick={() => (editorOpen = true)}>{editor.armed}</button>
        <button class="stop" aria-label="stop" data-acr-id="editor/stop" onclick={() => tapBar('editor/stop')}>×</button>
      </div>
    {/if}

    {#if touch.minimap && mapImage}
      <div class="minimap" data-acr-touch="minimap" data-acr-hit>
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

    {#if editor && !editorOpen}
      <div
        class="edge"
        role="presentation"
        data-acr-touch="edge"
        data-acr-hit
        style:--edge="{EDGE_PX}px"
        onpointerdown={edgeDown}
        onpointermove={edgeMove}
        onpointerup={edgeUp}
        onpointercancel={edgeUp}
      >
        <span class="tab"></span>
      </div>
    {/if}

    {#if editor && editorOpen}
      <EditorDrawer view={editor} onclose={() => (editorOpen = false)} />
    {/if}

    {#if hint}
      <div class="hint" role="status" data-acr-touch="hint">
        {hint}
      </div>
    {/if}

    <aside class="commands" data-acr-touch="commands" data-acr-hit>
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
    color: #e8cf93;
  }
  .upper {
    position: absolute;
    z-index: 1;
    top: 0;
    left: 0;
    right: 0;
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 4px;
    pointer-events: none;
  }
  .landscape .upper {
    --safe-right: 0px;
    right: calc(env(safe-area-inset-right) + var(--cols) * (var(--cell) + 4px) + 2 * var(--pad));
  }
  /* The bar: the resources' fold at the left, pause and menu at the right; it never wraps under them. */
  .top {
    position: relative;
    z-index: 2;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    box-sizing: border-box;
    padding: calc(2px + env(safe-area-inset-top)) calc(var(--pad) + var(--safe-right, env(safe-area-inset-right))) 2px calc(var(--pad) + env(safe-area-inset-left));
    background: linear-gradient(rgb(52 34 18 / 0.9), rgb(30 19 9 / 0.88));
    border-bottom: 1px solid #8a6a32;
    pointer-events: auto;
  }
  /* The panel under the bar: three columns on a narrow screen, six where there is room. */
  .resources {
    display: grid;
    grid-template-columns: repeat(var(--res-cols, 3), auto);
    justify-content: start;
    gap: 2px 14px;
    box-sizing: border-box;
    align-self: flex-start;
    margin: 0;
    padding: 4px var(--pad) 6px;
    list-style: none;
    background: rgb(30 19 9 / 0.86);
    border-radius: 0 0 8px 0;
    box-shadow: 0 3px 10px rgb(0 0 0 / 0.5);
    pointer-events: auto;
  }
  .hud[data-acr-size='medium'] .resources,
  .hud[data-acr-size='expanded'] .resources {
    --res-cols: 6;
  }
  .landscape .resources {
    --res-cols: 3;
  }
  .resources li {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .num {
    font: 400 18px/1 var(--touch-body);
    letter-spacing: 0.02em;
    color: #f3dc9c;
    text-shadow: 0 1px 2px #000;
    font-variant-numeric: lining-nums;
  }
  .buttons {
    display: flex;
    flex: none;
    gap: 6px;
  }
  .toggle {
    gap: 4px;
    min-width: 64px;
  }
  .scrim {
    position: absolute;
    inset: 0;
    padding: 0;
    background: none;
    border: 0;
    pointer-events: auto;
    -webkit-tap-highlight-color: transparent;
  }
  /* The speed popover under the bar, one slider with a stop per step. */
  .speed {
    position: absolute;
    top: 100%;
    right: var(--pad);
    box-sizing: border-box;
    width: min(340px, calc(100vw - 2 * var(--pad)));
    padding: 8px 14px 10px;
    background: #1e1309;
    border: 1px solid #8a6a32;
    border-radius: 0 0 8px 8px;
    box-shadow: 0 3px 10px rgb(0 0 0 / 0.5);
    pointer-events: auto;
  }
  .ends {
    display: flex;
    justify-content: space-between;
    font: 400 17px/1.2 var(--touch-body);
    color: #e8cf93;
  }
  .ends .now {
    color: #f3dc9c;
  }
  .speed input {
    display: block;
    width: 100%;
    height: 48px;
    margin: 4px 0 0;
    accent-color: #e2c27a;
    touch-action: pan-y;
  }
  .ticks {
    display: flex;
    justify-content: space-between;
    padding: 0 10px;
  }
  .ticks span {
    width: 2px;
    height: 8px;
    background: #8a6a32;
  }
  .ticks .normal {
    height: 14px;
    background: #e8cf93;
  }
  .ticks .on {
    background: #ffc840;
  }
  .bar,
  .fold {
    display: flex;
    align-items: center;
    justify-content: center;
    box-sizing: border-box;
    min-width: 44px;
    min-height: 44px;
    padding: 0;
    color: inherit;
    background: linear-gradient(#4f341b, #2b1b0e);
    border: 9px solid transparent;
    border-image: var(--acr-nine, none) 64 / 9px round;
    box-shadow: inset 0 0 14px rgb(98 62 22 / 0.5), 0 2px 6px rgb(0 0 0 / 0.55);
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
  .bar-glyph {
    display: block;
    width: 24px;
    height: 24px;
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
    top: calc(env(safe-area-inset-top) + 84px);
    left: 50%;
    transform: translateX(-50%);
    display: block;
    box-sizing: border-box;
    max-width: min(420px, calc(100% - 24px));
    padding: 8px 12px;
    font: 400 17px/1.35 var(--touch-body);
    color: var(--touch-ink);
    background: var(--touch-parchment) var(--acr-parchment, none) 0 0 / 200px 200px;
    border: 2px solid #6b4a22;
    box-shadow: 0 3px 10px rgb(0 0 0 / 0.6);
    pointer-events: none;
  }
  .landscape .hint {
    top: calc(env(safe-area-inset-top) + 56px);
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
  /* The original's pictures, enlarged by the engine's pixel-art scaler, are smoothed down to the cell. */
  .icon {
    --acr-image-rendering: auto;
  }
  .face {
    position: relative;
    display: block;
    width: calc(var(--cell) - 4px);
    height: calc(var(--cell) - 4px);
  }
  .face.dim {
    opacity: 0.45;
  }
  .over {
    position: absolute;
    inset: 0;
  }
  /* What a tap on the map places while the drawer is closed: its name opens the drawer, the cross stops placing. */
  .placing {
    display: flex;
    align-self: flex-start;
    margin-left: var(--pad);
    color: #f3dc9c;
    background: rgb(30 19 9 / 0.86);
    border: 1px solid #8a6a32;
    border-radius: 6px;
    box-shadow: 0 3px 10px rgb(0 0 0 / 0.5);
    pointer-events: auto;
  }
  .placing button {
    min-height: 44px;
    padding: 0 12px;
    font: 400 17px/1.1 var(--touch-body);
    color: inherit;
    background: none;
    border: 0;
    cursor: pointer;
    -webkit-tap-highlight-color: transparent;
  }
  .placing .stop {
    min-width: 44px;
    padding: 0;
    font-size: 26px;
    border-left: 1px solid #5b4126;
  }
  /* The strip at the right edge that starts the opening swipe; Android hands the page touches only inside data-acr-hit. */
  .edge {
    position: absolute;
    z-index: 3;
    top: 50%;
    right: 0;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    box-sizing: border-box;
    width: calc(var(--edge) + env(safe-area-inset-right));
    height: min(200px, 40vh);
    transform: translateY(-50%);
    touch-action: none;
    pointer-events: auto;
  }
  .edge .tab {
    width: 4px;
    height: 40px;
    margin-right: 3px;
    background: #e2c27a;
    border-radius: 2px 0 0 2px;
    box-shadow: 0 0 4px rgb(0 0 0 / 0.6);
  }
  .icon.selected {
    border-color: #ffc840;
  }
  .icon[aria-disabled='true'] {
    cursor: default;
  }
</style>
