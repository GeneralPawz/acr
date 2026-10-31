<script lang="ts">
  import Part from './Part.svelte';
  import type { WidgetView } from './protocol/WidgetView';

  let { widget }: { widget: WidgetView } = $props();
</script>

<!-- The classic skin only draws: the rect is for tests and the parity check, input goes through
     the viewport. -->
<div
  class="widget"
  data-acr-id={widget.id}
  data-acr-rect={widget.rect.join(',')}
  data-acr-hover={widget.hover || undefined}
>
  {#each widget.parts as part, i (i)}
    <Part {part} />
  {/each}
</div>

<style>
  .widget {
    position: absolute;
    left: 0;
    top: 0;
  }
</style>
