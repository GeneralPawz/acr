<script lang="ts">
  import { fly } from 'svelte/transition';
  import { ui } from '../context';
  import type { EditorButton } from '../protocol/EditorButton';
  import type { EditorCell } from '../protocol/EditorCell';
  import type { EditorSwatch } from '../protocol/EditorSwatch';
  import type { EditorView } from '../protocol/EditorView';
  import { intent, leftClick } from './intents';
  import Sprite from './Sprite.svelte';
  import { classify, closesDrawer } from './swipe';

  interface Props {
    view: EditorView;
    /** Close the drawer (the page's own state). */
    onclose: () => void;
  }
  let { view, onclose }: Props = $props();

  const { bridge } = ui();

  const rightClick = { t: 'click', button: 'right', mods: { shift: false, ctrl: false, alt: false, meta: false } } as const;
  /** A press that lasts this long is a long press (a right click, or the name of a cell). */
  const LONG_PRESS_MS = 450;
  /** Side of a cell of the lists; the art is 41 pixels. */
  const CELL = 52;

  // Transient: the nation list, the name of the cell last held, the swipe being tracked.
  let nations = $state(false);
  let hint = $state<string | null>(null);
  let hintTimer: ReturnType<typeof setTimeout> | undefined;
  let pressTimer: ReturnType<typeof setTimeout> | undefined;
  let long = false;
  let swipe: { id: number; x: number; y: number } | null = null;
  let swallow = false;

  const nation = $derived(view.entries.find((e) => e.on)?.text ?? '');

  function send(id: string, right = false) {
    intent(bridge, id, right ? rightClick : leftClick);
  }

  /** A tap on a button of the view; some ids also close the drawer, so the map is free to place on. */
  function tap(id: string) {
    send(id);
    if (closesDrawer(id)) onclose();
  }

  /** Starts a press; `onLong` runs once it lasts, and says whether it did something (else the release taps). */
  function press(onLong: () => boolean) {
    long = false;
    clearTimeout(pressTimer);
    pressTimer = setTimeout(() => (long = onLong()), LONG_PRESS_MS);
  }

  function release() {
    clearTimeout(pressTimer);
  }

  /** A tap that ends a long press is not a tap. */
  function tapUnless(id: string) {
    if (long) {
      long = false;
      return;
    }
    tap(id);
  }

  function holdCell(cell: EditorCell): boolean {
    if (cell.levels) {
      send(cell.id, true);
    } else {
      hint = cell.name;
      clearTimeout(hintTimer);
      hintTimer = setTimeout(() => (hint = null), 2500);
    }
    return true;
  }

  /** A long press on a colour cycles its team where teams fight. */
  function holdSwatch(s: EditorSwatch): boolean {
    if (s.teams) send(s.id, true);
    return s.teams;
  }

  function pickNation(b: EditorButton) {
    nations = false;
    send(b.id);
  }

  function down(e: PointerEvent) {
    swallow = false;
    swipe = { id: e.pointerId, x: e.clientX, y: e.clientY };
  }

  function move(e: PointerEvent) {
    if (!swipe || swipe.id !== e.pointerId) return;
    const got = classify(swipe, { x: e.clientX, y: e.clientY }, 'close');
    if (got === 'cancel') swipe = null;
    if (got === 'close') {
      swipe = null;
      swallow = true;
      release();
      onclose();
    }
  }

  function up() {
    swipe = null;
  }

  /** The click that ends a closing swipe goes nowhere. */
  function guard(e: MouseEvent) {
    if (!swallow) return;
    swallow = false;
    e.stopPropagation();
    e.preventDefault();
  }
</script>

{#snippet entry(b: EditorButton)}
  <button class="entry" class:on={b.on} data-acr-id={b.id} onclick={() => tap(b.id)}>{b.text}</button>
{/snippet}

<aside
  class="drawer"
  aria-label="editor"
  data-acr-hit
  data-acr-touch="editor"
  transition:fly|global={{ x: 420, duration: 160 }}
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
  onclickcapture={guard}
>
  <header class="head">
    <h2>{view.title}</h2>
    <button class="entry mode" class:on={view.toggle.on} data-acr-id={view.toggle.id} onclick={() => tap(view.toggle.id)}>
      {view.toggle.text}
    </button>
    <button class="entry close" aria-label="close" data-acr-id="drawer/close" onclick={onclose}>×</button>
  </header>

  <div class="scroll" data-acr-scroll>
    {#if !view.battle}
      <div class="swatches" role="group" aria-label="colours">
        {#each view.swatches as s (s.id)}
          <button
            class="swatch"
            class:on={s.on}
            aria-label={s.label}
            data-acr-id={s.id}
            style:--colour="rgb({s.colour.join(' ')})"
            onpointerdown={() => press(() => holdSwatch(s))}
            onpointerup={release}
            onpointercancel={release}
            onpointerleave={release}
            oncontextmenu={(e) => e.preventDefault()}
            onclick={() => tapUnless(s.id)}
          >
            {s.team ?? ''}
          </button>
        {/each}
      </div>
      <button class="entry nation" aria-haspopup="listbox" aria-expanded={nations} data-acr-id="drawer/nations" onclick={() => (nations = true)}>
        {nation}
      </button>
      {#if view.army.length > 0}
        <div class="chips" role="group" aria-label="army">
          {#each view.army as b (b.id)}
            {@render entry(b)}
          {/each}
        </div>
      {/if}
      {#each view.groups as group (group.header)}
        <h3>{group.header}</h3>
        <div class="cells" style:--cell="{CELL}px">
          {#each group.cells as cell (cell.id)}
            <button
              class="cell"
              class:on={cell.on}
              aria-label={cell.name}
              data-acr-id={cell.id}
              onpointerdown={() => press(() => holdCell(cell))}
              onpointerup={release}
              onpointercancel={release}
              onpointerleave={release}
              oncontextmenu={(e) => e.preventDefault()}
              onclick={() => tapUnless(cell.id)}
            >
              <Sprite parts={cell.parts} size={cell.size} scale={(CELL - 4) / cell.size[0]} />
            </button>
          {/each}
        </div>
      {/each}
    {/if}

    <div class="controls">
      {#each view.controls as c, i (i)}
        {#if c.t === 'stepper'}
          <div class="stepper">
            <button class="step" aria-label="less" data-acr-id={c.less} onclick={() => tap(c.less)}>‹</button>
            <span>{c.text}</span>
            <button class="step" aria-label="more" data-acr-id={c.more} onclick={() => tap(c.more)}>›</button>
          </div>
        {:else if c.t === 'button'}
          {@render entry({ id: c.id, text: c.text, on: c.on })}
        {:else if c.t === 'icon'}
          <button class="cell icon" class:on={c.on} aria-label={c.text} data-acr-id={c.id} onclick={() => tap(c.id)}>
            <Sprite parts={c.parts} size={c.size} scale={(CELL - 4) / c.size[0]} />
          </button>
        {:else}
          <p class="line">{c.text}</p>
        {/if}
      {/each}
    </div>
    {#if hint}
      <p class="line hint" role="status">{hint}</p>
    {/if}
    {#if view.status}
      <p class="line status" data-acr-touch="editor-status">{view.status}</p>
    {/if}
  </div>

  {#if nations}
    <button class="scrim" aria-label="close" data-acr-id="drawer/nations/close" onclick={() => (nations = false)}></button>
    <ul class="list" role="listbox" aria-label="nation" data-acr-scroll>
      {#each view.entries as b (b.id)}
        <li role="presentation">
          <button class="entry" class:on={b.on} role="option" aria-selected={b.on} data-acr-id={b.id} onclick={() => pickNation(b)}>{b.text}</button>
        </li>
      {/each}
    </ul>
  {/if}
  {#if view.levels.length > 0}
    <button class="scrim" aria-label="close" data-acr-id="editor/close" onclick={() => send('editor/close')}></button>
    <ul class="list" aria-label="levels" data-acr-scroll>
      {#each view.levels as b (b.id)}
        <li>{@render entry(b)}</li>
      {/each}
    </ul>
  {/if}
</aside>

<style>
  .drawer {
    position: absolute;
    z-index: 4;
    top: var(--bar-h, 0px);
    right: 0;
    bottom: 0;
    display: flex;
    flex-direction: column;
    box-sizing: border-box;
    width: min(88vw, 420px);
    padding: 0 env(safe-area-inset-right) env(safe-area-inset-bottom) 0;
    color: var(--touch-ink);
    background: var(--touch-parchment) var(--acr-parchment, none) 0 0 / 200px 200px;
    border-left: 9px solid transparent;
    border-image: var(--acr-nine, none) 64 / 0 0 0 9px round;
    box-shadow: -3px 0 12px rgb(0 0 0 / 0.55);
    pointer-events: auto;
    --acr-image-rendering: auto;
  }
  .head {
    display: flex;
    flex: none;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    background: rgb(42 24 9 / 0.12);
    border-bottom: 1px solid #6b4a22;
  }
  h2 {
    flex: 1;
    margin: 0;
    font: 400 20px/1.1 var(--touch-caps);
  }
  h3 {
    margin: 10px 0 4px;
    font: 400 17px/1.1 var(--touch-caps);
    border-bottom: 1px solid #6b4a22;
  }
  .scroll {
    flex: 1;
    min-height: 0;
    padding: 8px 10px 12px;
    overflow-y: auto;
    overscroll-behavior: contain;
    touch-action: pan-y;
  }
  button {
    font: 400 17px/1.1 var(--touch-body);
    color: inherit;
    cursor: pointer;
    -webkit-tap-highlight-color: transparent;
    -webkit-touch-callout: none;
  }
  .entry {
    box-sizing: border-box;
    min-width: 44px;
    min-height: 44px;
    padding: 4px 10px;
    text-align: left;
    background: rgb(42 24 9 / 0.1);
    border: 1px solid #6b4a22;
  }
  .entry.on {
    color: #f3dc9c;
    background: var(--touch-leather);
    border-color: #8a6a32;
  }
  .mode {
    text-align: center;
  }
  .close {
    padding: 0;
    font-size: 26px;
    text-align: center;
  }
  .nation {
    display: block;
    width: 100%;
    margin-top: 8px;
  }
  .nation::after {
    content: ' ▾';
  }
  .swatches {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 4px;
  }
  .swatch {
    box-sizing: border-box;
    min-height: 44px;
    padding: 0;
    font-size: 20px;
    color: #fff;
    text-shadow: 0 1px 2px #000;
    background: var(--colour);
    border: 2px solid #3a2612;
  }
  .swatch.on {
    border-color: #ffc840;
    box-shadow: 0 0 0 2px #2a1809;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 8px;
  }
  .cells {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(var(--cell), 1fr));
    justify-items: center;
    gap: 4px;
  }
  .cell {
    box-sizing: border-box;
    display: flex;
    align-items: center;
    justify-content: center;
    width: var(--cell, 52px);
    height: var(--cell, 52px);
    padding: 0;
    background: #1a1007;
    border: 2px solid #3a2612;
  }
  .cell.on {
    border-color: #ffc840;
    box-shadow: 0 0 0 1px #2a1809;
  }
  .controls {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-top: 10px;
  }
  .controls > .entry {
    width: 100%;
  }
  .controls > .icon {
    align-self: flex-start;
  }
  .stepper {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .stepper span {
    flex: 1;
    text-align: center;
  }
  .step {
    flex: none;
    box-sizing: border-box;
    width: 44px;
    height: 44px;
    padding: 0;
    font-size: 26px;
    background: rgb(42 24 9 / 0.1);
    border: 1px solid #6b4a22;
  }
  .line {
    margin: 2px 0;
    font: 400 15px/1.25 var(--touch-body);
  }
  .status {
    margin-top: 8px;
    color: var(--touch-ink-soft);
  }
  .scrim {
    position: absolute;
    inset: 0;
    padding: 0;
    background: rgb(18 10 4 / 0.55);
    border: 0;
  }
  .list {
    position: absolute;
    inset: 8px 12px auto;
    z-index: 1;
    max-height: calc(100% - 16px);
    display: flex;
    flex-direction: column;
    gap: 3px;
    margin: 0;
    padding: 8px;
    overflow-y: auto;
    overscroll-behavior: contain;
    touch-action: pan-y;
    list-style: none;
    background: var(--touch-parchment) var(--acr-parchment, none) 0 0 / 200px 200px;
    border: 2px solid #6b4a22;
    box-shadow: 0 3px 10px rgb(0 0 0 / 0.6);
  }
  .list .entry {
    width: 100%;
  }
</style>
