#!/usr/bin/env bash
# Builds the Android WebView spike (docs/webui.md, "Android spike") as its own app, package
# io.github.generalpawz.acr.webviewspike: `tools/android/build-webview-spike.sh [out.apk]`.
# Same environment as build-apk.sh (source tools/android-env.sh); the example is built as a
# cdylib and a small dex holds the Java side. It never uses the game's package id.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
out=${1:-$root/target/android/WebViewSpike.apk}
bt=$(ls -d "$ANDROID_HOME"/build-tools/* | sort -V | tail -1)
platform=$(ls -d "$ANDROID_HOME"/platforms/android-* | sort -V | tail -1)
tc=$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin
target=${CARGO_TARGET_DIR:-$root/target}
work=$target/android/spike-work
spike=$root/tools/android/webview-spike
rm -rf "$work" && mkdir -p "$work/lib/arm64-v8a" "$work/classes"
cargo build --release -p acr-game --example webview_spike_android --target aarch64-linux-android \
  --manifest-path "$root/Cargo.toml"
cp "$target/aarch64-linux-android/release/examples/libwebview_spike_android.so" "$work/lib/arm64-v8a/"
"$tc/llvm-strip" --strip-debug "$work/lib/arm64-v8a/libwebview_spike_android.so"
javac -source 8 -target 8 -Xlint:-options -classpath "$platform/android.jar" -d "$work/classes" \
  "$spike"/java/io/github/generalpawz/acr/webviewspike/*.java
"$bt/d8" --lib "$platform/android.jar" --min-api 28 --output "$work" $(find "$work/classes" -name '*.class')
"$bt/aapt2" link -o "$work/base.apk" -I "$platform/android.jar" --manifest "$spike/AndroidManifest.xml"
(cd "$work" && zip -q -0 -r base.apk lib classes.dex)
"$bt/zipalign" -f -p 16384 "$work/base.apk" "$work/aligned.apk"
key=${ACR_ANDROID_KEY:-$HOME/.android/acr-debug.keystore}
mkdir -p "$(dirname "$out")"
"$bt/apksigner" sign --ks "$key" --ks-pass pass:android --ks-key-alias acr --out "$out" "$work/aligned.apk"
echo "$out"
