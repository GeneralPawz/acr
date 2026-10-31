# Android build

The game runs on Android as a `NativeActivity` (`crates/acr-game/src/android.rs`, winit's
`android-native-activity`). It plays the original's game files; none are in the app.

## Toolchain

`source ~/projects/acr/tools/android-env.sh` points `JAVA_HOME`, `ANDROID_HOME` and
`ANDROID_NDK_HOME` at the JDK, the SDK (build-tools, platform 36) and the NDK, and sets the
cargo linker for `aarch64-linux-android`. The Rust target is `rustup target add aarch64-linux-android`.

```sh
source ~/projects/acr/tools/android-env.sh
cargo clippy -p acr-game -p acr-render -p acr-app --target aarch64-linux-android --all-targets -- -D warnings
```

## Build and install

```sh
ACR_ANDROID_ABIS=arm64-v8a bash tools/android/build-apk.sh   # target/android/AmericanConquest.apk
adb install -r target/android/AmericanConquest.apk
adb logcat -s acr                                              # the game's log
```

`ACR_ANDROID_ABIS="arm64-v8a x86_64"` adds the emulator's ABI. The APK is signed with a local
debug key (`~/.android/acr-debug.keystore`, created on first use).

## Launcher icon

`tools/android/make-icon.py` draws an adaptive icon (API 26+, with a monochrome layer for themed
icons) plus round legacy PNGs at build time into the work dir; nothing generated is committed. The
original only has its emblem at 32x32 (`Ac.ico`; the exe resources and the menu art hold nothing
larger) and upscaling it gives a blob, so the gold "AC" monogram is redrawn: `tools/android/monogram.svg`
holds its calligraphic strokes (our own drawing, traced from the 32x32 icon), which the script
stamps with a broad nib and shades in brass with a dark outline. The backdrop (dark bronze
gradient, thin gold ring) is drawn at 432x432 (108 dp). Needs Python with Pillow and numpy;
`ACR_ICON_PREVIEW=path.png` writes a preview of the masked result next to the old icon.

## Game data

The app looks for the original's folder (the one with `dmcr.exe`) in this order:

1. `Android/data/io.github.generalpawz.acr/files/American Conquest/` (or `files/` itself) on the
   shared storage. Copy the game there with `adb push` or a file manager.
2. `American Conquest/` at the top of the shared storage. This needs "All files access"; when no
   folder is found, the app opens that system settings page and looks again on return.

Saves, profiles and settings are kept in the app's own folder (`data_dir()` in `android.rs`).

## Shaders

Adreno drivers miscompile a vector indexed by a runtime value, so the shaders select lanes with
`component()` (a mask and a dot product) instead of `v[i]`. Keep that when changing a shader.

## Controls

Touch gestures are in `crates/acr-game/src/touch.rs` (its header lists them): tap, drag, long
press and swipe, pinch, and the pause gestures. In touch mode a long press on a production icon is the Ctrl+click (endless production, #438), and
a building chosen to place shows a blinking outline in the middle of the window: a tap moves it, a
double tap (350 ms, 40 dp) places it, Escape or a long press cancels (#439). The touch top bar (`top_bar=touch`) is the
default on Android.

### Mouse and keyboard

With a mouse and keyboard attached the game plays as on the desktop and follows the last input
used (`crates/acr-game/src/input_mode.rs`): a finger puts it in touch mode (the touch top bar,
the gestures, no cursor); a mouse move, button or wheel puts it in mouse mode (the classic top
bar, the game's own cursor, the wheel zooms). An explicit `top_bar=` in `settings.txt` stays in
both modes. Android's own pointer is hidden (`PointerIcon.TYPE_NULL` on the decor and content
views, `hide_system_pointer` in `android.rs`). Hardware keys reach the game as key events;
Escape is also the back key.

winit 0.30 turns every `MotionEvent` into a `Touch` and drops hover, buttons and the wheel, and
winit's master skips mouse pointers as well. The workspace therefore patches winit to the fork
`GeneralPawz/winit`, branch `acr/android-mouse` (v0.30.13 plus the mouse mapping; see `[patch.crates-io]`
in the root `Cargo.toml`). Drop the patch when winit reports Android mice.

## Battery

Issue #440. The game presents with `PresentMode::AutoVsync` (the display's rate), and the
settings below, all in `settings.txt` (no options page), spend less:

| Key | Values | Default | |
|---|---|---|---|
| `frame_cap` | `off`, `30`, `60` | `off` | at most that many frames per second; `off` is the display's rate |
| `render_scale` | `100`, `85`, `75`, `50` | unset: `100` (below it the view shows less of the world in sprite mode, so it is an opt-in until the camera compensates) | the share of the window's size the frame is drawn at, then stretched over the window (see below) |
| `power_save` | `on`, `off` | unset | `on` caps at 30 fps; unset follows Android's Game Mode: `BATTERY` caps, anything else does not (`GameManager.getGameMode()` through JNI, API 31+, read on every resume; `game_mode_battery` in `android.rs`) |

The cap is the stricter of `frame_cap` and the saver's 30 fps. It skips the redraw requests
until the next frame time (`pacing.rs`): the event loop sleeps (`ControlFlow::WaitUntil`),
nothing waits in a loop, and the vsync present still aligns the frame to a refresh. A frame
is due at 90 % of the interval, so the present waits for the refresh instead of missing it.

### Render on demand

On Android (and with `power_save=on` elsewhere; `power_save=off` switches it off) the rate drops
to 5 fps once a frame has looked like the one before for 30 frames in a row with no input
(`pacing.rs`: `QUIET_FRAMES`, `IDLE_FPS`). Any window event, a finger on the screen, a message
or a change of the picture restores the normal rate at once. What "looks the same" means
(`Game::look_signature` in `lib.rs`): the hash of the frame's interface draws (image, rect, tint:
a menu, a dialog, the HUD, the cursor), and in a game also the camera, and while the game runs
(not paused, no dialog) the position and animation frame of every unit and projectile. These
are never quiet, so they keep the full rate: the intro movies, a loading game, the Svelte UI,
the free camera, particles, awake ragdolls, weather or a storm at sea, and a running game
drawn with 3D models (wind and water move all the time; paused, they hold still). When a
signature cannot be told, the frame counts as changed. A game that stands still lets the
simulation catch up with up to 0.2 s of ticks on its next frame.

### Render scale

`render_scale=100|85|75|50` draws the frame at that share of the window's size into an offscreen
target (`acr_render::upscale`, `Upscaler`) and stretches it over the swapchain with one
triangle: linear filtering, nearest at 50 %. `Gpu::size()` (`acr-app`) is the render size, so
the layout, the scene and the depth buffers follow it, and the input positions (mouse, touch,
and the dp of the touch gestures) are converted from window to render pixels
(`Game::to_render`). The **whole frame is scaled, the interface included**: the menus and the
HUD are laid out on the canvas of the render size and are as soft as the world; drawing them
at full size on top would need the world and the interface in two passes with two layouts,
which the pipeline does not have. The shader indexes no vector component at run time
(Adreno). With 100 % nothing changes: no offscreen target, no extra pass. Without the key it is
100 % everywhere: below it sprites keep their pixel size, so the view shows less of the world,
and it stays an opt-in until the camera compensates for it.

The game lays out for the render size like for a smaller window, and the sprite view shows as
much of the world as the render size has pixels (as the original does at a lower resolution):
at 75 % a sprite view shows 75 % of the width and height of the world it showed at 100 %, with the
sprites at the same pixel size, then stretched.

### Performance hints (ADPF)

On API 31+ the game creates a `PerformanceHintManager` session for the main thread through JNI
(`HintSession` in `android.rs`, once, at the first resume; NDK `APerformanceHint_*` would need
API 33). Every frame reports the thread's work (the redraw's time without the wait for the
swapchain) and, when it changes, the budget: the interval of the rate in effect (the cap, the
idle 5 fps) or the display's refresh interval (`pacing::frame_budget`), so the scheduler can use
efficient cores and lower clocks while the work is small. Without a manager or a session
(older Android, a refusal) it logs `performance hints: not available` at info level and
nothing changes; a failing call stops the hints. logcat tag `acr` shows
`performance hints: session created`.

### Audio

`acr-audio` plays through cpal 0.18, whose Android backend is AAudio (not Oboe). It opens the
stream with the default performance mode (the low-latency mode is cpal's `realtime` feature,
which is off) and cpal has no power-saving mode to ask for. The lever it offers is the callback
size: by default AAudio calls the mixer every burst (a few milliseconds, hundreds of wakeups a
second), so on Android `output.rs` asks for `BufferSize::Fixed` of 40 ms of audio, rounded up to
64 frames (1920 at 48 kHz, `android_buffer_frames`): about 25 wakeups a second, for a 40 ms
delay of sounds that a strategy game does not notice. A fixed size also turns off cpal's dynamic
buffer tuning after underruns; if the device refuses the fixed size, the stream opens with the
default (logcat `audio: no large callback`). The log line `audio: callback of Fixed(1920)
frames` shows the size in use. Not measured: the effect on battery and on underruns is for
the phone (`audio stream` warnings in logcat).
