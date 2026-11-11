import { mount } from 'svelte';
import { androidBridge, onAndroid } from './lib/android';
import type { Bridge } from './lib/bridge';
import { nativeBridge, webBridge } from './lib/bridge';
import { FontCache } from './lib/fonts';
import type { FromUi } from './lib/protocol/FromUi';
import { UiState } from './lib/state.svelte';
import { HUD, SCREENS } from './screens/registry';
import Root from './lib/Root.svelte';

/** Version of `acr-ui-protocol` this bundle speaks. */
export const PROTOCOL = 1;

/** Mounts the UI into `target` and announces it to the game. */
export function mountUi(target: HTMLElement, bridge: Bridge) {
  const state = new UiState(bridge);
  bridge.onMessage((message) => state.receive(message));
  const app = mount(Root, { target, props: { bridge, fonts: new FontCache(bridge), model: state } });
  const viewport = (): Pick<Extract<FromUi, { t: 'viewport' }>, 'viewport' | 'dpr'> => ({
    viewport: [window.innerWidth, window.innerHeight],
    dpr: window.devicePixelRatio || 1,
  });
  window.addEventListener('resize', () => bridge.send({ t: 'viewport', ...viewport() }));
  bridge.send({ t: 'ready', protocol: PROTOCOL, screens: [...SCREENS], hud: [...HUD], ...viewport() });
  return app;
}

export { androidBridge, nativeBridge, onAndroid, webBridge };
export type { Bridge };
