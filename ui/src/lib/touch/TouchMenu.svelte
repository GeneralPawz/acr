<script lang="ts">
  import './theme.css';
  import { ui } from '../context';
  import Sprite from './Sprite.svelte';
  import { goBack, intent, leftClick } from './intents';
  import TextField from './TextField.svelte';
  import { sheetBlocks, type Cell } from './sheet';
  import type { WidgetView } from '../protocol/WidgetView';

  const { bridge, model } = ui();
  // The header carries the screen's title (the widget Rust keys `title`) and its back arrow, so the sheet
  // leaves both out: the title (and a static text repeating it), and the back or cancel widget the arrow presses.
  const title = $derived(model.widgets.find((w) => w.key === 'title' && w.shown && w.label?.trim()));
  const back = $derived(model.screen?.back ?? null);
  const blocks = $derived(sheetBlocks(model.widgets.filter((w) => w !== title && w.id !== back && !(title && w.kind === 'text' && w.label?.trim().toLowerCase() === title.label?.trim().toLowerCase()))));
  // The combo box whose options are shown (transient: Rust owns the choice), and its screen: a list left
  // open does not follow the player to the next screen.
  let picked = $state<{ screen: string | undefined; id: string } | null>(null);
  const picking = $derived(picked !== null && picked.screen === model.screen?.name ? picked.id : null);

  function pick(widget: WidgetView, option: number) {
    picked = null;
    intent(bridge, widget.id, { t: 'select_option', option });
  }
</script>

<!-- A control with the text that names it above it. -->
{#snippet entry(cell: Cell)}
  {@const widget = cell.widget}
  {@const v = widget.value}
  {#if cell.caption}
    <span class="caption">{cell.caption.label}</span>
  {/if}
  {#if widget.kind === 'text'}
    <p class="plain">{widget.label}</p>
  {:else if widget.kind === 'button' || widget.kind === 'picture'}
    <button class="entry" class:pager={widget.key === 'next_page'} disabled={!widget.enabled} data-acr-key={widget.key} data-acr-id={widget.id} onclick={() => intent(bridge, widget.id, leftClick)}>
      {#if widget.label}
        <span class="text">{widget.label}</span>
      {:else}
        <Sprite parts={widget.parts} size={[widget.rect[2], widget.rect[3]]} origin={[widget.rect[0], widget.rect[1]]} scale={1.5} />
        {#if cell.mark}
          <span class="mark">{cell.mark.label}</span>
        {/if}
      {/if}
    </button>
  {:else if widget.kind === 'strip'}
    <button
      class="entry check"
      role="checkbox"
      aria-checked={v?.t === 'check' && v.on}
      disabled={!widget.enabled}
      data-acr-key={widget.key}
      data-acr-id={widget.id}
      onclick={() => intent(bridge, widget.id, leftClick)}
    >
      <span class="box" class:on={v?.t === 'check' && v.on}></span>
      <span class="text">{widget.label}</span>
    </button>
  {:else if v?.t === 'list'}
    <div class="list" role="listbox" data-acr-scroll data-acr-key={widget.key} data-acr-id={widget.id}>
      {#each v.rows as row, i (i)}
        <button
          class="entry row"
          role="option"
          aria-selected={v.selected === i}
          class:selected={v.selected === i}
          onclick={() => intent(bridge, widget.id, { t: 'select_row', row: i })}
        >
          <span class="text">{row}</span>
        </button>
      {/each}
    </div>
  {:else if v?.t === 'combo'}
    <button class="entry combo" data-acr-key={widget.key} data-acr-id={widget.id} onclick={() => (picked = picking === widget.id ? null : { screen: model.screen?.name, id: widget.id })}>
      <span class="text">{v.options[v.selected] ?? ''}</span>
      <span class="caret" aria-hidden="true"></span>
    </button>
    {#if picking === widget.id}
      <!-- The choices as a full-width list over the sheet; a tap beside it closes it. -->
      <div class="picker" data-acr-hit role="presentation" onclick={() => (picked = null)}>
        <div class="options" role="listbox" tabindex="-1" data-acr-scroll onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
          {#each v.options as option, i (i)}
            <button class="entry row" role="option" aria-selected={v.selected === i} class:selected={v.selected === i} onclick={() => pick(widget, i)}>
              <span class="text">{option}</span>
            </button>
          {/each}
        </div>
      </div>
    {/if}
  {:else if v?.t === 'range'}
    <input
      class="range"
      type="range"
      min="0"
      max={v.max}
      value={v.value}
      data-acr-key={widget.key}
      data-acr-id={widget.id}
      aria-label={widget.key ?? widget.id}
      onchange={(e) => intent(bridge, widget.id, { t: 'set_value', value: Number(e.currentTarget.value) })}
    />
  {:else if v?.t === 'input'}
    <TextField
      text={v.text}
      capacity={v.capacity}
      id={widget.id}
      key={widget.key}
      onchange={(text) => intent(bridge, widget.id, { t: 'set_text', text })}
    />
  {:else if v?.t === 'viewer'}
    <div class="viewer" data-acr-scroll data-acr-id={widget.id}>
      <p>{v.text}</p>
    </div>
  {/if}
{/snippet}

<!-- The whole sheet is interactive (`data-acr-hit`): the host gives it every touch, so it scrolls. -->
<main class="sheet" data-acr-touch="sheet" data-acr-hit data-acr-screen={model.screen?.name}>
  <div class="backdrop" aria-hidden="true"></div>
  {#if title || back}
    <header class="bar">
      {#if back}
        <button class="back" type="button" aria-label="Back" data-acr-back data-acr-id={back} onclick={() => goBack(bridge)}>
          <span class="arrow" aria-hidden="true"></span>
        </button>
      {/if}
      {#if title}
        <h1 class="title">{(title.label ?? '').trim()}</h1>
      {/if}
    </header>
  {/if}
  <div class="scroll" data-acr-scroll>
  <ul class="items">
    {#each blocks as block (block.t === 'cell' ? block.cell.widget.id : block.t === 'section' ? block.heading.id : block.cells[0].widget.id)}
      {#if block.t === 'section'}
        <li class="section"><h2>{block.heading.label}</h2></li>
      {:else if block.t === 'cell'}
        <li class="item {block.cell.widget.kind}">
          {@render entry(block.cell)}
        </li>
      {:else}
        <li class="item card">
          {#each block.cells as c (c.widget.id)}
            <div class="cell {c.widget.kind}">
              {@render entry(c)}
            </div>
          {/each}
        </li>
      {/if}
    {/each}
  </ul>
  </div>
</main>

<style>
  /* The sheet: the main menu's painting, dimmed and blurred, behind leather, parchment and gold. */
  .sheet {
    position: fixed;
    inset: 0;
    box-sizing: border-box;
    height: 100dvh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    color: var(--touch-gold);
    background: #130c06;
    pointer-events: auto;
    --acr-image-rendering: auto;
  }
  .backdrop {
    position: absolute;
    inset: 0;
    overflow: hidden;
  }
  /* A little larger than the sheet, so that the blur does not fade at the edges. */
  .backdrop::before {
    content: '';
    position: absolute;
    inset: -12px;
    background: #130c06 var(--acr-backdrop, none) 50% 38% / cover no-repeat;
    filter: blur(2.5px) brightness(0.52) saturate(0.85) sepia(0.12);
  }
  .backdrop::after {
    content: '';
    position: absolute;
    inset: 0;
    background: radial-gradient(ellipse at 50% 45%, transparent 35%, rgb(10 5 2 / 0.55) 100%);
  }
  /* The sticky top bar: dark leather over a wood grain, the gold title, a gilt edge. */
  .bar {
    position: relative;
    z-index: 1;
    flex: none;
    display: flex;
    align-items: center;
    gap: 4px;
    box-sizing: border-box;
    min-height: calc(60px + env(safe-area-inset-top));
    padding: env(safe-area-inset-top) max(8px, env(safe-area-inset-right)) 0 max(4px, env(safe-area-inset-left));
    background:
      repeating-linear-gradient(90deg, rgb(255 220 150 / 0.035) 0 2px, transparent 2px 7px),
      linear-gradient(#4a3019, #2b1b0e 70%, #22150a);
    border-bottom: 2px solid #8a6a32;
    box-shadow:
      0 1px 0 #1a0f06,
      0 4px 14px rgb(0 0 0 / 0.65),
      inset 0 1px 0 rgb(255 225 160 / 0.22);
  }
  .back {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 48px;
    height: 48px;
    padding: 0;
    background: transparent;
    border: 0;
    border-radius: 50%;
    cursor: pointer;
    -webkit-tap-highlight-color: transparent;
  }
  .back:active {
    background: rgb(226 194 122 / 0.2);
  }
  .arrow {
    width: 14px;
    height: 14px;
    margin-left: 6px;
    box-sizing: border-box;
    border: solid var(--touch-gold);
    border-width: 0 0 3px 3px;
    transform: rotate(45deg);
    filter: drop-shadow(0 1px 1px #000);
  }
  .title {
    flex: 1;
    min-width: 0;
    margin: 0;
    /* The arrow's width again on the right, so the title is centred. */
    padding-right: 48px;
    overflow: hidden;
    font: 400 clamp(16px, 5vw, 24px) / 1.15 var(--touch-caps);
    letter-spacing: 0.03em;
    text-align: center;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: #f0d48e;
    text-shadow:
      0 1px 0 #1a0f06,
      0 2px 6px rgb(0 0 0 / 0.8);
  }
  .bar:not(:has(.back)) .title {
    padding-right: 0;
  }
  /* The native touch scrolling of the sheet: momentum, and no chaining into the game below. */
  .scroll {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow-x: hidden;
    overflow-y: auto;
    overscroll-behavior: contain;
    -webkit-overflow-scrolling: touch;
    touch-action: pan-y;
    padding: 16px max(14px, env(safe-area-inset-right)) max(28px, env(safe-area-inset-bottom)) max(14px, env(safe-area-inset-left));
  }
  .items {
    display: flex;
    flex-direction: column;
    gap: 12px;
    box-sizing: border-box;
    max-width: 560px;
    margin: 0 auto;
    padding: 0;
    list-style: none;
  }
  /* The heading of a group of controls: small capitals in gold over a fading rule. */
  .section {
    margin: 18px 0 0;
    padding: 0 2px 7px;
    background: linear-gradient(90deg, var(--touch-gold-dim), rgb(176 141 76 / 0)) bottom / 100% 1px no-repeat;
    text-align: left;
  }
  .section:first-child {
    margin-top: 0;
  }
  .section h2 {
    margin: 0;
    font: 400 18px/1.2 var(--touch-caps);
    letter-spacing: 0.1em;
    color: var(--touch-gold);
    text-shadow: 0 1px 4px rgb(0 0 0 / 0.9);
  }
  .item {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: center;
    gap: 6px;
    box-sizing: border-box;
    min-height: 24px;
  }
  .plain {
    width: 100%;
    margin: 0;
    font-size: 17px;
    line-height: 1.4;
    color: #f0e2c0;
    text-shadow: 0 1px 4px rgb(0 0 0 / 0.9);
  }
  /* An entry is a sheet of parchment in the original's frame; a press sinks it into the page. */
  .entry {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    box-sizing: border-box;
    width: 100%;
    min-height: 52px;
    padding: 6px 12px;
    color: var(--touch-ink);
    font: 400 19px/1.2 var(--touch-body);
    background: var(--touch-parchment) var(--acr-parchment, none) 0 0 / 200px 200px;
    border: 9px solid transparent;
    border-image: var(--acr-nine, none) 64 / 9px round;
    border-radius: 0;
    box-shadow:
      inset 0 0 22px rgb(98 62 22 / 0.5),
      0 3px 8px rgb(0 0 0 / 0.55);
    cursor: pointer;
    -webkit-tap-highlight-color: transparent;
    transition:
      transform 90ms ease-out,
      box-shadow 90ms ease-out;
  }
  /* The page switch of a dialog with several pages (the in-game options): leather and gold, not paper. */
  .pager {
    color: #f3dc9c;
    background: linear-gradient(#4f341b, #2b1b0e);
    border-image: none;
    border-width: 1px;
    border-color: var(--touch-gold-dim);
    border-radius: 4px;
    text-shadow: 0 1px 2px #000;
  }
  .text {
    min-width: 0;
    overflow-wrap: anywhere;
    text-align: center;
  }
  .entry:disabled {
    opacity: 0.55;
    filter: saturate(0.6);
  }
  .entry:active:not(:disabled) {
    transform: translateY(2px);
    box-shadow:
      inset 0 0 28px rgb(70 40 12 / 0.75),
      0 1px 3px rgb(0 0 0 / 0.55);
    filter: brightness(0.9);
  }
  .entry:focus-visible,
  .back:focus-visible {
    outline: 2px solid var(--touch-gold);
    outline-offset: 2px;
  }
  /* The rows of a list and the options of a drop-down: one parchment, ruled between the rows. */
  .list,
  .options {
    box-sizing: border-box;
    width: 100%;
    overflow-y: auto;
    overscroll-behavior: contain;
    touch-action: pan-y;
    background: var(--touch-parchment) var(--acr-parchment, none) 0 0 / 200px 200px;
    box-shadow:
      inset 0 0 22px rgb(98 62 22 / 0.5),
      0 0 0 2px #6b4a22,
      0 3px 10px rgb(0 0 0 / 0.55);
  }
  .list {
    max-height: 44dvh;
  }
  .row {
    min-height: 52px;
    padding: 4px 14px;
    font-size: 18px;
    background: transparent;
    border: 0;
    border-bottom: 1px solid rgb(90 63 34 / 0.35);
    border-image: none;
    box-shadow: none;
    justify-content: flex-start;
  }
  .row .text {
    text-align: left;
  }
  .row:last-child {
    border-bottom: 0;
  }
  .row:active:not(:disabled) {
    transform: none;
    box-shadow: none;
    background: rgb(90 55 20 / 0.22);
  }
  /* The chosen row: dark leather with gold type and a gilt bar at its edge. */
  .row.selected {
    color: #f3dc9c;
    background: linear-gradient(#4a3019, #33200f);
    box-shadow: inset 4px 0 0 var(--touch-gold);
    text-shadow: 0 1px 2px #000;
  }
  .row.selected .text::before {
    content: '\2767\00a0';
    color: var(--touch-gold);
  }
  /* The open drop-down: a sheet from the bottom edge, in the same parchment. */
  .picker {
    position: fixed;
    inset: 0;
    z-index: 2;
    display: flex;
    align-items: flex-end;
    background: rgb(8 4 2 / 0.7);
  }
  .options {
    max-height: 70dvh;
    padding-bottom: env(safe-area-inset-bottom);
    border-top: 3px solid var(--touch-gold-dim);
    border-radius: 14px 14px 0 0;
    box-shadow: 0 -6px 24px rgb(0 0 0 / 0.7);
    -webkit-overflow-scrolling: touch;
  }
  /* A text viewer is a page of the original's paper. */
  .viewer {
    box-sizing: border-box;
    width: 100%;
    max-height: 44dvh;
    overflow-y: auto;
    padding: 14px 16px;
    color: var(--touch-ink);
    background: var(--touch-parchment) var(--acr-parchment, none) 0 0 / 200px 200px;
    box-shadow:
      inset 0 0 26px rgb(98 62 22 / 0.5),
      0 0 0 2px #6b4a22,
      0 3px 10px rgb(0 0 0 / 0.55);
    touch-action: pan-y;
  }
  .viewer p {
    margin: 0;
    font-size: 17px;
    line-height: 1.45;
    hyphens: auto;
    text-align: left;
    white-space: pre-line;
  }
  .box {
    flex: none;
    width: 24px;
    height: 24px;
    box-sizing: border-box;
    border: 2px solid var(--touch-ink-soft);
    border-radius: 3px;
    background: rgb(255 244 214 / 0.55);
    box-shadow: inset 0 1px 3px rgb(60 35 10 / 0.5);
  }
  .box.on {
    background:
      linear-gradient(45deg, transparent 0 40%, var(--touch-ink) 40% 58%, transparent 58%) 3px 5px / 8px 12px no-repeat,
      linear-gradient(135deg, transparent 0 40%, var(--touch-ink) 40% 58%, transparent 58%) 7px 2px / 12px 14px no-repeat,
      rgb(255 244 214 / 0.55);
  }
  .check {
    justify-content: flex-start;
  }
  .check .text {
    text-align: left;
  }
  .combo {
    justify-content: space-between;
  }
  .caret {
    flex: none;
    width: 0;
    height: 0;
    border: 7px solid transparent;
    border-top-color: var(--touch-ink-soft);
    margin-top: 7px;
  }
  /* A slider: a leather groove and a gilt thumb. */
  .range {
    width: 100%;
    min-height: 44px;
    margin: 0;
    /* A vertical drag scrolls the sheet; only a horizontal one moves the slider. */
    touch-action: pan-y;
    background: transparent;
    -webkit-appearance: none;
    appearance: none;
  }
  .range::-webkit-slider-runnable-track {
    height: 10px;
    background: linear-gradient(#1c1108, #3d2813);
    border: 1px solid #8a6a32;
    border-radius: 6px;
    box-shadow: inset 0 1px 3px #000;
  }
  .range::-moz-range-track {
    height: 8px;
    background: linear-gradient(#1c1108, #3d2813);
    border: 1px solid #8a6a32;
    border-radius: 6px;
  }
  .range::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 26px;
    height: 26px;
    margin-top: -9px;
    background: radial-gradient(circle at 35% 30%, #f6e2a4, #c79a45 60%, #7a5420);
    border: 1px solid #3a2610;
    border-radius: 50%;
    box-shadow: 0 2px 5px rgb(0 0 0 / 0.7);
  }
  .range::-moz-range-thumb {
    width: 24px;
    height: 24px;
    background: radial-gradient(circle at 35% 30%, #f6e2a4, #c79a45 60%, #7a5420);
    border: 1px solid #3a2610;
    border-radius: 50%;
  }
  .caption {
    display: block;
    width: 100%;
    margin: 0 0 3px;
    font: 400 14px/1.2 var(--touch-caps);
    letter-spacing: 0.08em;
    color: #d8bd84;
    text-shadow: 0 1px 3px rgb(0 0 0 / 0.9);
    text-align: left;
  }
  .item,
  .cell {
    flex-direction: column;
    align-items: stretch;
  }
  /* A table row: its controls side by side, each under its column header, on a leather panel. */
  .card {
    flex-direction: row;
    flex-wrap: wrap;
    align-items: flex-end;
    padding: 10px;
    background: linear-gradient(rgb(60 38 18 / 0.78), rgb(32 20 9 / 0.82));
    border: 1px solid #8a6a32;
    border-radius: 4px;
    box-shadow:
      inset 0 1px 0 rgb(255 225 160 / 0.15),
      0 3px 10px rgb(0 0 0 / 0.5);
  }
  .cell {
    display: flex;
    flex: 1 1 130px;
    min-width: 0;
  }
  .cell.picture,
  .cell.button {
    flex: 0 1 84px;
  }
  /* The text a picture field draws over itself (the team's X). */
  .mark {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font: 400 22px/1 var(--touch-caps);
    color: #fff3cf;
    text-shadow: 0 1px 3px #000;
    pointer-events: none;
  }
</style>
