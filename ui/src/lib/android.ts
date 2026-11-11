import type { Bridge } from './bridge';
import { nativeBridge } from './bridge';

/** What the Android host's Java side puts on `window` (`WebHost.Bridge`, `@JavascriptInterface`). */
interface AcrAndroid {
  post(json: string): void;
  /** The page's interactive rectangles as `x,y,w,h;...` in CSS pixels; touches outside go to the game. */
  regions(csv: string): void;
}

/** Where the host serves assets (`webui/android.rs`). */
export const ANDROID_ASSETS = 'https://acr.local/asset/v1';

/** The elements the page owns the touches of: the stage of a screen the UI draws (`Stage.svelte`). */
export const HIT = '[data-acr-hit]';

/** Whether the page runs in the Android host. */
export function onAndroid(): boolean {
  const acr = (window as unknown as { acr?: Partial<AcrAndroid> }).acr;
  return typeof acr?.post === 'function' && typeof acr.regions === 'function';
}

/** The rectangles of the elements matching `HIT`, as the host reads them. */
export function regionsOf(root: ParentNode = document): string {
  return [...root.querySelectorAll(HIT)]
    .map((e) => {
      const r = e.getBoundingClientRect();
      return [r.x, r.y, r.width, r.height].map((n) => Math.round(n * 10) / 10).join(',');
    })
    .join(';');
}

/** Reports the regions to `report` whenever they may have changed (and once now); returns the stop function. */
export function watchRegions(report: (csv: string) => void, root: HTMLElement = document.body): () => void {
  let last: string | undefined;
  let queued = false;
  const measure = () => {
    queued = false;
    const csv = regionsOf(root);
    if (csv !== last) {
      last = csv;
      report(csv);
    }
  };
  const schedule = () => {
    if (queued) return;
    queued = true;
    requestAnimationFrame(measure);
  };
  const observer = new MutationObserver(schedule);
  observer.observe(root, { subtree: true, childList: true, attributes: true, attributeFilter: ['style', 'data-acr-hit'] });
  window.addEventListener('resize', schedule);
  measure();
  return () => {
    observer.disconnect();
    window.removeEventListener('resize', schedule);
  };
}

/** The Android host's bridge: messages over `acr.post`, assets from `https://acr.local`, and the
 * touch regions reported to Java (browser and Linux pages never call this). */
export function androidBridge(): Bridge {
  const acr = (window as unknown as { acr: AcrAndroid }).acr;
  watchRegions((csv) => acr.regions(csv));
  return nativeBridge(ANDROID_ASSETS, (json) => acr.post(json));
}
