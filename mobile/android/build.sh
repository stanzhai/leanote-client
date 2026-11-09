#!/usr/bin/env bash
# leanote Android 构建（issue 10）：Rust cdylib + APK 一条龙。
# 用法: ./build.sh [--release]   （需 ANDROID_NDK_HOME 或默认 NDK 28.2）
set -euo pipefail
cd "$(dirname "$0")/gradle"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$HOME/Library/Android/sdk/ndk/28.2.13676358}"
./gradlew $([ "${1:-}" = "--release" ] && echo buildRustRelease || echo buildRustDebug) assembleDebug
echo "APK: $(pwd)/app/build/outputs/apk/debug/app-debug.apk"
