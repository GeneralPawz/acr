import type { FromUi } from './protocol/FromUi';
import type { ToUi } from './protocol/ToUi';

/** The way between the UI and the game; one per host. */
export interface Bridge {
  send(message: FromUi): void;
  onMessage(callback: (message: ToUi) => void): void;
  /** An object URL (or other URL) of an asset path such as `bmp/...`; cached. */
  assetUrl(path: string): Promise<string>;
  assetBytes(path: string): Promise<Uint8Array>;
  /** Starts the game's audio; called inside the user gesture. */
  unlockAudio?(): void;
}

/** The functions the wasm module exports for the UI (`webui/web.rs`). */
export interface WebExports {
  acr_ui_connect(callback: (json: string) => void): void;
  acr_ui_send(json: string): void;
  acr_ui_asset(path: string): Promise<Uint8Array>;
  acr_audio_unlock(): void;
}

const MIME: Record<string, string> = { png: 'image/png', json: 'application/json' };

/** The browser build: the UI and the game share a page and call each other directly. */
export function webBridge(wasm: WebExports): Bridge {
  const urls = new Map<string, Promise<string>>();
  const bytes = new Map<string, Promise<Uint8Array>>();
  let listener: (message: ToUi) => void = () => {};
  wasm.acr_ui_connect((json) => listener(JSON.parse(json) as ToUi));
  const fetchBytes = (path: string) => {
    let pending = bytes.get(path);
    if (!pending) {
      pending = wasm.acr_ui_asset(path);
      // A failed asset is not cached: the next ask tries again.
      pending.catch(() => bytes.delete(path));
      bytes.set(path, pending);
    }
    return pending;
  };
  return {
    send: (message) => wasm.acr_ui_send(JSON.stringify(message)),
    onMessage: (callback) => {
      listener = callback;
    },
    assetBytes: fetchBytes,
    assetUrl(path) {
      let url = urls.get(path);
      if (!url) {
        const type = MIME[path.split('.').pop() ?? ''] ?? 'image/png';
        url = fetchBytes(path).then((data) => URL.createObjectURL(new Blob([data as BlobPart], { type })));
        url.catch(() => urls.delete(path));
        urls.set(path, url);
      }
      return url;
    },
    unlockAudio: () => wasm.acr_audio_unlock(),
  };
}

interface NativeWindow {
  ipc: { postMessage(message: string): void };
  __acr_ui?: { recv(json: string): void };
}

/** The native webview: messages over wry's IPC, assets from the custom protocol. Untested
 * until the native host exists (tracking issue #316, section 3). */
export function nativeBridge(assetBase = 'acr://localhost/asset/v1'): Bridge {
  const host = window as unknown as NativeWindow;
  let listener: (message: ToUi) => void = () => {};
  host.__acr_ui = { recv: (json) => listener(JSON.parse(json) as ToUi) };
  const urls = new Map<string, Promise<string>>();
  const fetchBytes = async (path: string) => {
    const response = await fetch(`${assetBase}/${path}`);
    if (!response.ok) throw new Error(`${response.status} ${response.headers.get('x-acr-error') ?? path}`);
    return new Uint8Array(await response.arrayBuffer());
  };
  return {
    send: (message) => host.ipc.postMessage(JSON.stringify(message)),
    onMessage: (callback) => {
      listener = callback;
    },
    assetBytes: fetchBytes,
    assetUrl(path) {
      let url = urls.get(path);
      if (!url) {
        url = Promise.resolve(`${assetBase}/${path}`);
        urls.set(path, url);
      }
      return url;
    },
  };
}
