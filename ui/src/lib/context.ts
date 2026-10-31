import { getContext } from 'svelte';
import type { Bridge } from './bridge';
import type { FontCache } from './fonts';
import type { UiState } from './state.svelte';

export interface UiContext {
  bridge: Bridge;
  fonts: FontCache;
  model: UiState;
}

export const KEY = Symbol('acr-ui');

export function ui(): UiContext {
  return getContext<UiContext>(KEY);
}
