#!/bin/sh
set -eu
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
SDK=${WEBOS_NDK:-"$ROOT/.build/toolchain/arm-webos-linux-gnueabi_sdk-buildroot"}
SYSROOT="$SDK/arm-webos-linux-gnueabi/sysroot"
SDL_DIR=${WEBOS_SDL_RUNTIME:-"$ROOT/.build/sdl-webos"}
OUT="$ROOT/.build/hardware-wallpaper"
mkdir -p "$OUT"
"$SDK/bin/arm-webos-linux-gnueabi-g++" --sysroot="$SYSROOT" -O2 -fPIC -shared \
 -std=c++17 -D__WEBOS__ -I"$SYSROOT/usr/include" -I"$SYSROOT/usr/include/glib-2.0" -I"$SYSROOT/usr/lib/glib-2.0/include" -I"$SDL_DIR/include/SDL2" \
 -o "$OUT/libluma_cinema.so" "$ROOT/native/cinema/luma_cinema.cpp" \
 -L"$SYSROOT/usr/lib" -L"$SDL_DIR/lib" \
 -lumedia_api_c -lumedia_api -lums_connector -lpbnjson_cpp -lpbnjson_c -lglib-2.0 -lSDL2 -lpthread -ldl \
 -Wl,-soname,libluma_cinema.so -Wl,-rpath,\$ORIGIN
"$SDK/bin/arm-webos-linux-gnueabi-readelf" -h "$OUT/libluma_cinema.so" | grep -E '(Class|Machine|Flags):'
