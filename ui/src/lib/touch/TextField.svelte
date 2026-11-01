<script lang="ts">
  import GameLabel from './GameLabel.svelte';
  import type { FontRef } from './sheet';

  interface Props {
    font: FontRef;
    /** The text Rust holds. */
    text: string;
    capacity: number;
    id: string;
    key?: string;
    onchange: (text: string) => void;
  }
  let { font, text, capacity, id, key, onchange }: Props = $props();

  // What was typed since Rust last sent the text; the game font draws it as it is typed.
  let typed = $state<string | null>(null);
  let focused = $state(false);
  const shown = $derived(typed ?? text);
  $effect(() => {
    void text;
    typed = null;
  });
</script>

<!-- The text is the game font's glyphs at the label size. The real input is only the soft keyboard's
     target: it lies over the field, invisible, so that a tap focuses it. -->
<div class="field" class:focused>
  <GameLabel {...font} text={shown} scale={1.5} fit maxHeight={30} />
  {#if focused}<span class="cursor" aria-hidden="true"></span>{/if}
  <input
    type="text"
    maxlength={Math.max(capacity - 1, 0)}
    value={text}
    data-acr-key={key}
    data-acr-id={id}
    aria-label={key ?? id}
    oninput={(e) => (typed = e.currentTarget.value)}
    onfocus={() => (focused = true)}
    onblur={() => {
      focused = false;
      typed = null;
    }}
    onchange={(e) => onchange(e.currentTarget.value)}
  />
</div>

<style>
  .field {
    position: relative;
    display: flex;
    align-items: center;
    box-sizing: border-box;
    width: 100%;
    min-height: 48px;
    padding: 6px 12px;
    background: #2a1e12;
    border: 2px solid #8a6d3b;
  }
  .field.focused {
    border-color: #d9b46a;
  }
  .cursor {
    flex: none;
    width: 3px;
    height: 24px;
    margin-left: 2px;
    background: #d9b46a;
  }
  input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    padding: 0;
    opacity: 0;
    /* 16px or more keeps a phone browser from zooming in on focus. */
    font-size: 16px;
  }
</style>
