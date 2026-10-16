#!/bin/sh
set -eu
root=/work/device-build/sysroot
exec arm-linux-gnueabihf-gcc --sysroot="$root" \
 -B"$root/usr/lib/arm-linux-gnueabihf/" \
 -L"$root/lib/arm-linux-gnueabihf" -L"$root/usr/lib/arm-linux-gnueabihf" \
 -Wl,-rpath-link,"$root/lib/arm-linux-gnueabihf:$root/usr/lib/arm-linux-gnueabihf:$root/opt/vc/lib" \
 "$@"
