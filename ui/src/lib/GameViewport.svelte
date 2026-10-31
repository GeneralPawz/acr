<script lang="ts">
  import { onMount } from 'svelte';
  import { ui } from './context';
  import { MoveCoalescer, browserKey, buttonOf, modsOf, wheelLines } from './input';

  const { bridge, model } = ui();
  let { dpr }: { dpr: number } = $props();

  // Raw input in window (physical) pixels. Nothing is hit-tested here: the game owns the
  // widgets and the cursor's meaning.
  const out = new MoveCoalescer((m) => bridge.send(m));
  let cursorUrl = $state<string | undefined>();

  $effect(() => {
    const cursor = model.cursor;
    if (!cursor) {
      cursorUrl = undefined;
      return;
    }
    let live = true;
    bridge.assetUrl(cursor.url).then(
      (url) => {
        if (live) cursorUrl = `url(${url}) ${cursor.hotspot[0]} ${cursor.hotspot[1]}, auto`;
      },
      (e) => console.error('cursor:', e),
    );
    return () => {
      live = false;
    };
  });

  function position(e: PointerEvent): { x: number; y: number } {
    return { x: e.clientX * dpr, y: e.clientY * dpr };
  }

  function pointer(e: PointerEvent, phase: 'move' | 'down' | 'up' | 'cancel') {
    const message = { t: 'pointer', phase, ...position(e), button: phase === 'move' ? null : buttonOf(e.button), mods: modsOf(e) } as const;
    if (phase === 'move') out.move(message);
    else out.send(message);
  }

  function down(e: PointerEvent) {
    bridge.unlockAudio?.();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    pointer(e, 'down');
  }

  function wheel(e: WheelEvent) {
    e.preventDefault();
    out.send({ t: 'wheel', x: e.clientX * dpr, y: e.clientY * dpr, lines: wheelLines(e), mods: modsOf(e) });
  }

  function fullscreen() {
    // Inside the gesture; forwarded events could not do this.
    if (document.fullscreenElement) void document.exitFullscreen();
    else void document.documentElement.requestFullscreen();
  }

  function key(e: KeyboardEvent, down: boolean) {
    if (browserKey(e)) return;
    if (down && (e.key === 'F11' || (e.key === 'Enter' && e.altKey))) {
      e.preventDefault();
      return fullscreen();
    }
    if (e.key === 'F11') return e.preventDefault();
    e.preventDefault();
    if (down) bridge.unlockAudio?.();
    out.send({ t: 'key', key: e.key, code: e.code, down, repeat: e.repeat, mods: modsOf(e) });
  }

  onMount(() => {
    // The game's canvas must not also receive keys.
    (document.activeElement as HTMLElement | null)?.blur?.();
    const onDown = (e: KeyboardEvent) => key(e, true);
    const onUp = (e: KeyboardEvent) => key(e, false);
    const onBlur = () => out.send({ t: 'blur' });
    window.addEventListener('keydown', onDown);
    window.addEventListener('keyup', onUp);
    window.addEventListener('blur', onBlur);
    return () => {
      window.removeEventListener('keydown', onDown);
      window.removeEventListener('keyup', onUp);
      window.removeEventListener('blur', onBlur);
    };
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="viewport"
  data-acr-viewport
  style:cursor={cursorUrl}
  onpointermove={(e) => pointer(e, 'move')}
  onpointerdown={down}
  onpointerup={(e) => pointer(e, 'up')}
  onpointercancel={(e) => pointer(e, 'cancel')}
  oncontextmenu={(e) => e.preventDefault()}
  onwheel={wheel}
></div>

<style>
  .viewport {
    position: absolute;
    inset: 0;
    pointer-events: auto;
    touch-action: none;
    user-select: none;
  }
</style>
