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
a building chosen to place shows a blinking outline in the middle of the window: a tap moves it there, a drag (started anywhere) slides it along with the finger, a
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
