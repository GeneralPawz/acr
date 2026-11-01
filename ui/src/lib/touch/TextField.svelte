<script lang="ts">
  interface Props {
    /** The text Rust holds. */
    text: string;
    capacity: number;
    id: string;
    key?: string;
    onchange: (text: string) => void;
  }
  let { text, capacity, id, key, onchange }: Props = $props();

  // What was typed since Rust last sent the text; drawn in the skin's type as it is typed.
  let typed = $state<string | null>(null);
  let focused = $state(false);
  const shown = $derived(typed ?? text);
  $effect(() => {
    void text;
    typed = null;
  });
</script>

<!-- The text is drawn here, in the skin's type. The real input is only the soft keyboard's
     target: it lies over the field, invisible, so that a tap focuses it. -->
<div class="field" class:focused>
  <span class="shown">{shown}</span>
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
  /* An inset of ink-dark paper: the field takes what is written into it. */
  .field {
    position: relative;
    display: flex;
    align-items: center;
    box-sizing: border-box;
    width: 100%;
    min-height: 52px;
    padding: 6px 14px;
    color: var(--touch-ink);
    font: 400 19px/1.2 var(--touch-body);
    background: #e4d2a8 var(--acr-parchment, none) 0 0 / 200px 200px;
    border: 2px solid #6b4a22;
    border-radius: 3px;
    box-shadow:
      inset 0 2px 8px rgb(70 40 12 / 0.6),
      0 1px 0 rgb(255 225 160 / 0.2);
  }
  .field.focused {
    border-color: var(--touch-gold);
    box-shadow:
      inset 0 2px 8px rgb(70 40 12 / 0.6),
      0 0 0 2px rgb(226 194 122 / 0.55);
  }
  .shown {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: pre;
  }
  .cursor {
    flex: none;
    width: 2px;
    height: 24px;
    margin-left: 1px;
    background: var(--touch-ink);
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
