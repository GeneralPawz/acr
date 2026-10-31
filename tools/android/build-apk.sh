#!/usr/bin/env bash
# Builds the game as an Android app: `tools/android/build-apk.sh [out.apk]`. Phones are arm64;
# `ACR_ANDROID_ABIS="arm64-v8a x86_64"` adds the emulator's ABI.
# Needs the Android SDK (build-tools, platform 36), the NDK and the aarch64-linux-android
# Rust target; ANDROID_HOME and ANDROID_NDK_HOME point to them. Signed with a local debug key.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
out=${1:-$root/target/android/AmericanConquest.apk}
bt=$(ls -d "$ANDROID_HOME"/build-tools/* | sort -V | tail -1)
platform=$(ls -d "$ANDROID_HOME"/platforms/android-* | sort -V | tail -1)
tc=$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin
abis=${ACR_ANDROID_ABIS:-arm64-v8a}
target=${CARGO_TARGET_DIR:-$root/target}
work=$root/target/android/work
rm -rf "$work" && mkdir -p "$work/res/mipmap"
for abi in $abis; do
  case $abi in
    arm64-v8a) triple=aarch64-linux-android ;;
    x86_64) triple=x86_64-linux-android ;;
    *) echo "unknown ABI $abi" >&2; exit 1 ;;
  esac
  var=${triple//-/_}
  export "CARGO_TARGET_${var^^}_LINKER=$tc/${triple}28-clang" "CC_$var=$tc/${triple}28-clang" "AR_$var=$tc/llvm-ar"
  cargo build --release -p acr-game --lib --target "$triple" --manifest-path "$root/Cargo.toml"
  mkdir -p "$work/lib/$abi"
  cp "$target/$triple/release/libacr_game.so" "$work/lib/$abi/"
  "$tc/llvm-strip" --strip-debug "$work/lib/$abi/libacr_game.so"
done
# The icon embedded in the Windows build, as the launcher icon.
python3 -c "from PIL import Image; Image.open('$root/crates/acr-game/Ac.ico').convert('RGBA').resize((96, 96), Image.LANCZOS).save('$work/res/mipmap/icon.png')"
"$bt/aapt2" compile --dir "$work/res" -o "$work/res.zip"
"$bt/aapt2" link -o "$work/base.apk" -I "$platform/android.jar" --manifest "$root/tools/android/AndroidManifest.xml" "$work/res.zip"
(cd "$work" && zip -q -0 -r base.apk lib)
"$bt/zipalign" -f -p 16384 "$work/base.apk" "$work/aligned.apk"
key=${ACR_ANDROID_KEY:-$HOME/.android/acr-debug.keystore}
if [ ! -f "$key" ]; then
  mkdir -p "$(dirname "$key")"
  keytool -genkeypair -keystore "$key" -storepass android -keypass android -alias acr \
    -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=ACR debug" >/dev/null
fi
mkdir -p "$(dirname "$out")"
"$bt/apksigner" sign --ks "$key" --ks-pass pass:android --ks-key-alias acr --out "$out" "$work/aligned.apk"
echo "$out"
