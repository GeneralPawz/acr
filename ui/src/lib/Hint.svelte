<script lang="ts">
  import GameText from './GameText.svelte';

  let { hint }: { hint: string } = $props();

  // `Menu::draw`: SmallYellowFont, centred at (440, 745).
</script>

<GameText font="SmallYellowFont" set={1} text={hint} x={440} y={745} center />
