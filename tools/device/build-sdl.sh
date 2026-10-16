#!/bin/sh
set -eu
sysroot=/work/device-build/sysroot
prefix=/work/device-build/sdl
mkdir -p /work/device-build/sdl-build
cd /work/device-build/sdl-build
export CC=arm-linux-gnueabihf-gcc
export CXX=arm-linux-gnueabihf-g++
export CFLAGS="--sysroot=$sysroot -B$sysroot/usr/lib/arm-linux-gnueabihf/ -O2 -march=armv7-a -mfpu=neon-vfpv4 -mfloat-abi=hard -I$sysroot/opt/vc/include -I$sysroot/opt/vc/include/interface/vcos/pthreads -I$sysroot/opt/vc/include/interface/vmcs_host/linux"
export CXXFLAGS="$CFLAGS"
export LDFLAGS="--sysroot=$sysroot -L$sysroot/opt/vc/lib -L$sysroot/lib/arm-linux-gnueabihf -L$sysroot/usr/lib/arm-linux-gnueabihf -Wl,-rpath,/opt/vc/lib"
export PKG_CONFIG_SYSROOT_DIR="$sysroot"
export PKG_CONFIG_LIBDIR="$sysroot/usr/lib/arm-linux-gnueabihf/pkgconfig:$sysroot/usr/share/pkgconfig"
/sdl/configure --host=arm-linux-gnueabihf --prefix="$prefix" \
 --disable-static --enable-shared --enable-video-rpi --disable-video-x11 \
 --disable-video-wayland --disable-video-kmsdrm --disable-video-vulkan \
 --disable-pulseaudio --disable-jack --disable-pipewire --disable-ibus --disable-fcitx
make -j4
make install
