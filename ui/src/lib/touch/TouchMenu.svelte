<script lang="ts">
  import { ui } from '../context';
  import GameLabel from './GameLabel.svelte';
  import Sprite from './Sprite.svelte';
  import { intent, leftClick } from './intents';
  import TextField from './TextField.svelte';
  import { fontOf, sheetBlocks, words, type Cell } from './sheet';
  import type { WidgetView } from '../protocol/WidgetView';

  const { bridge, model } = ui();
  const blocks = $derived(sheetBlocks(model.widgets));
  // The combo box whose options are shown (transient: Rust owns the choice).
  let picking = $state<string | null>(null);

  function pick(widget: WidgetView, option: number) {
    picking = null;
    intent(bridge, widget.id, { t: 'select_option', option });
  }
</script>

<!-- A control with the text that names it above it. -->
{#snippet entry(cell: Cell)}
  {@const widget = cell.widget}
  {@const font = fontOf(widget)}
  {@const v = widget.value}
  {#if cell.caption}
    <span class="caption"><GameLabel {...fontOf(cell.caption)} text={cell.caption.label ?? ''} scale={1} fit maxHeight={20} /></span>
  {/if}
  {#if widget.kind === 'text'}
    <GameLabel {...font} text={widget.label ?? ''} scale={1.5} fit maxHeight={30} />
  {:else if widget.kind === 'button' || widget.kind === 'picture'}
    <button class="entry" disabled={!widget.enabled} data-acr-key={widget.key} data-acr-id={widget.id} onclick={() => intent(bridge, widget.id, leftClick)}>
      {#if widget.label}
        <GameLabel {...font} text={widget.label} scale={1.5} fit maxHeight={30} />
      {:else}
        <Sprite parts={widget.parts} size={[widget.rect[2], widget.rect[3]]} origin={[widget.rect[0], widget.rect[1]]} scale={1.5} />
        {#if cell.mark}
          <span class="mark"><GameLabel {...fontOf(cell.mark)} text={cell.mark.label ?? ''} scale={1.5} /></span>
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
      <GameLabel {...font} text={widget.label ?? ''} scale={1.5} fit maxHeight={30} />
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
          <GameLabel {...font} text={row} scale={1.5} fit maxHeight={26} />
        </button>
      {/each}
    </div>
  {:else if v?.t === 'combo'}
    <button class="entry combo" data-acr-key={widget.key} data-acr-id={widget.id} onclick={() => (picking = picking === widget.id ? null : widget.id)}>
      <GameLabel {...font} text={v.options[v.selected] ?? ''} scale={1.5} fit maxHeight={30} />
      <span class="caret" aria-hidden="true"></span>
    </button>
    {#if picking === widget.id}
      <div class="options" role="listbox" data-acr-scroll>
        {#each v.options as option, i (i)}
          <button class="entry row" role="option" aria-selected={v.selected === i} class:selected={v.selected === i} onclick={() => pick(widget, i)}>
            <GameLabel {...font} text={option} scale={1.5} fit maxHeight={26} />
          </button>
        {/each}
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
      {font}
      text={v.text}
      capacity={v.capacity}
      id={widget.id}
      key={widget.key}
      onchange={(text) => intent(bridge, widget.id, { t: 'set_text', text })}
    />
  {:else if v?.t === 'viewer'}
    <div class="viewer" data-acr-scroll data-acr-id={widget.id}>
      {#each words(v.text) as word, i (i)}
        <GameLabel {...font} text={word} scale={1.5} />
      {/each}
    </div>
  {/if}
{/snippet}

<main class="sheet" data-acr-touch="sheet" data-acr-screen={model.screen?.name}>
  <ul class="items">
    {#each blocks as block (block.t === 'cell' ? block.cell.widget.id : block.cells[0].widget.id)}
      {#if block.t === 'cell'}
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
</main>

<style>
  .sheet {
    position: fixed;
    inset: 0;
    box-sizing: border-box;
    height: 100dvh;
    overflow-y: auto;
    padding: max(12px, env(safe-area-inset-top)) max(12px, env(safe-area-inset-right)) max(12px, env(safe-area-inset-bottom))
      max(12px, env(safe-area-inset-left));
    background: #1c150e;
    pointer-events: auto;
    touch-action: pan-y;
  }
  /* The original's text is drawn dark for its painted panels. */
  .sheet :global(.label) {
    filter: brightness(2.6);
  }
  .items {
    display: flex;
    flex-direction: column;
    gap: 10px;
    box-sizing: border-box;
    max-width: 560px;
    margin: 0 auto;
    padding: 0;
    list-style: none;
  }
  @media (orientation: landscape) {
    .items {
      display: grid;
      grid-template-columns: 1fr 1fr;
      max-width: 960px;
    }
    .item.list,
    .item.viewer,
    .item.card {
      grid-column: 1 / -1;
    }
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
  .entry {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    box-sizing: border-box;
    width: 100%;
    min-height: 44px;
    padding: 4px 10px;
    color: inherit;
    font: inherit;
    background: #3a2a18 var(--acr-nine-fill, none);
    border: 12px solid transparent;
    border-image: var(--acr-nine, none) 64 / 12px round;
    border-radius: 0;
    cursor: pointer;
    -webkit-tap-highlight-color: transparent;
  }
  .entry:disabled {
    opacity: 0.5;
  }
  .entry:active:not(:disabled),
  .entry.selected {
    background-color: #5b4126;
  }
  .row {
    min-height: 40px;
    padding: 2px 10px;
    border-width: 0 0 1px;
    border-image: none;
    border-bottom: 1px solid #5b4126;
  }
  .list,
  .options,
  .viewer {
    box-sizing: border-box;
    width: 100%;
    max-height: 40dvh;
    overflow-y: auto;
    background: #2a1e12;
    touch-action: pan-y;
  }
  .viewer {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 10px;
    padding: 8px;
  }
  .box {
    flex: none;
    width: 22px;
    height: 22px;
    box-sizing: border-box;
    border: 3px solid #d9b46a;
    background: transparent;
  }
  .box.on {
    background: #d9b46a;
  }
  .caret {
    flex: none;
    width: 0;
    height: 0;
    border: 8px solid transparent;
    border-top-color: #d9b46a;
    margin-top: 8px;
  }
  .range {
    width: 100%;
    min-height: 44px;
    accent-color: #d9b46a;
  }
  .caption {
    display: block;
    width: 100%;
    margin: 0 0 2px;
    opacity: 0.8;
    text-align: left;
  }
  .item,
  .cell {
    flex-direction: column;
    align-items: stretch;
  }
  /* A table row: its controls side by side, each under its column header. */
  .card {
    flex-direction: row;
    flex-wrap: wrap;
    align-items: flex-end;
    padding: 8px;
    background: #2a1e12;
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
  .entry {
    position: relative;
  }
  /* The text a picture field draws over itself (the team's X). */
  .mark {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
  }
</style>
