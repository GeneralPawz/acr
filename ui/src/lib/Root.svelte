<script lang="ts">
  import { setContext } from 'svelte';
  import type { Bridge } from './bridge';
  import { KEY } from './context';
  import type { FontCache } from './fonts';
  import type { UiState } from './state.svelte';
  import Ui from './Ui.svelte';

  let { bridge, fonts, model }: { bridge: Bridge; fonts: FontCache; model: UiState } = $props();

  // The services never change for a mounted UI.
  // svelte-ignore state_referenced_locally
  setContext(KEY, { bridge, fonts, model });
</script>

<Ui />
