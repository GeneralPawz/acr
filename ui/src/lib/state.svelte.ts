import type { Bridge } from './bridge';
import { applyHud, applyWidgets, isSnapshot } from './patch';
import type { CanvasView } from './protocol/CanvasView';
import type { CursorView } from './protocol/CursorView';
import type { EditorView } from './protocol/EditorView';
import type { HudGroup } from './protocol/HudGroup';
import type { Mode } from './protocol/Mode';
import type { ScreenPatch } from './protocol/ScreenPatch';
import type { SkinSetting } from './protocol/SkinSetting';
import type { Stage } from './protocol/Stage';
import type { TouchHud } from './protocol/TouchHud';
import type { ToUi } from './protocol/ToUi';
import type { WidgetView } from './protocol/WidgetView';

/** The screen without its widgets. */
export type ScreenInfo = Omit<ScreenPatch, 'widgets'>;

/** What the game last told the UI (the view model). The UI keeps nothing else but transient
 * state. Large arrays are `$state.raw`: patches replace them. */
export class UiState {
  mode = $state.raw<Mode>('intro');
  loading = $state.raw<number | null>(null);
  stage = $state.raw<Stage | null>(null);
  screen = $state.raw<ScreenInfo | null>(null);
  widgets = $state.raw<WidgetView[]>([]);
  cursor = $state.raw<CursorView | null>(null);
  /** The groups of the in-game interface, in stacking order. */
  hud = $state.raw<HudGroup[]>([]);
  /** The touch skin's view of the interface (the last `touch` topic). */
  touch = $state.raw<TouchHud | null>(null);
  /** The test editor's drawer (the last `editor` topic); `null`: not an editor game. */
  editor = $state.raw<EditorView | null>(null);
  /** Game protocol version of the last `Hello`. */
  protocol = $state.raw<number | null>(null);
  /** The player's `ui_skin` of the last `Hello`. */
  skinSetting = $state.raw<SkinSetting>('auto');

  #rev = 0;
  #awaitingSnapshot = false;
  readonly #bridge: Bridge;

  constructor(bridge: Bridge) {
    this.#bridge = bridge;
  }

  /** The canvas of the menus, when the game told. */
  get menu(): CanvasView | null {
    return this.stage?.menu ?? null;
  }

  receive(message: ToUi): void {
    if (message.t === 'hello') {
      this.protocol = message.protocol;
      this.skinSetting = message.skin ?? 'auto';
      this.#rev = 0;
      this.#awaitingSnapshot = false;
      return;
    }
    // The game says why it refused an intent; nothing changed, so the log is all there is to do.
    if (message.t === 'rejected') {
      console.warn(`intent ${message.seq} refused: ${message.reason}`);
      return;
    }
    const patch = message;
    if (this.#awaitingSnapshot && !isSnapshot(patch)) return;
    if (!this.#awaitingSnapshot && patch.rev !== this.#rev + 1) {
      // A lost patch: ask for everything again and ignore patches until it arrives.
      this.#awaitingSnapshot = true;
      this.#bridge.send({ t: 'resync' });
      return;
    }
    const snapshot = this.#awaitingSnapshot;
    this.#awaitingSnapshot = false;
    this.#rev = patch.rev;
    if (patch.mode != null) this.mode = patch.mode;
    if (patch.loading !== undefined) this.loading = patch.loading;
    if (patch.stage != null) this.stage = patch.stage;
    if (patch.cursor !== undefined) this.cursor = patch.cursor;
    if (patch.touch != null) this.touch = patch.touch;
    if (patch.editor !== undefined) this.editor = patch.editor;
    if (patch.hud != null) this.hud = applyHud(snapshot ? [] : this.hud, patch.hud);
    if (patch.screen != null) {
      const { widgets, ...info } = patch.screen;
      this.screen = info;
      this.widgets = applyWidgets(this.widgets, widgets);
    }
  }
}
