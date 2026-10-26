#!/bin/sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
if [ -z "${WEBOS_NDK:-}" ]; then WEBOS_NDK="$ROOT/.build/toolchain/arm-webos-linux-gnueabi_sdk-buildroot"; fi
SYSROOT="$WEBOS_NDK/arm-webos-linux-gnueabi/sysroot"
CC="$WEBOS_NDK/bin/arm-webos-linux-gnueabi-gcc"
OUT="$ROOT/.build/hardware-wallpaper"
mkdir -p "$OUT"
# webOS 6 DirectMedia v2. Starfish BufferStream freezes after one displayed
# video frame on this C1; NDL owns the video plane and advances its clock.
"$CC" --sysroot="$SYSROOT" -O2 -fPIC -D__WEBOS__ \
    -I"$SYSROOT/usr/include/libndl-media" \
    -shared -Wl,-soname,libluma_hwplayer.so \
    -o "$OUT/libluma_hwplayer.so" "$ROOT/native/smp/luma_ndlplayer.c" \
    -L"$SYSROOT/usr/lib" -lNDL_directmedia -lpthread -ldl
export PKG_CONFIG_SYSROOT_DIR="$SYSROOT"
export PKG_CONFIG_LIBDIR="$SYSROOT/usr/lib/pkgconfig:$SYSROOT/usr/share/pkgconfig"
GST_CFLAGS=$(pkg-config --cflags gstreamer-app-1.0)
GST_LIBS=$(pkg-config --libs gstreamer-app-1.0)
"$CC" --sysroot="$SYSROOT" -O2 $GST_CFLAGS "$ROOT/native/smp/luma_demux.c" -o "$OUT/luma-demux" $GST_LIBS
file "$OUT/libluma_hwplayer.so" "$OUT/luma-demux"
