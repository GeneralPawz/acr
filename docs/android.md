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

## Game data

The app looks for the original's folder (the one with `dmcr.exe`) in this order:

1. `Android/data/io.github.generalpawz.acr/files/American Conquest/` (or `files/` itself) on the
   shared storage. Copy the game there with `adb push` or a file manager.
2. `American Conquest/` at the top of the shared storage. This needs "All files access"; when no
   folder is found, the app opens that system settings page and looks again on return.

Saves, profiles and settings are kept next to the game folder.

## Shaders

Adreno drivers miscompile a vector indexed by a runtime value, so the shaders select lanes with
`component()` (a mask and a dot product) instead of `v[i]`. Keep that when changing a shader.

## Controls

See the touch gestures in `crates/acr-game/src/touch.rs`.
