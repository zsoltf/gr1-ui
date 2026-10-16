#!/bin/sh
set -eu
export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER=/work/tools/device/linker.sh
export CARGO_TARGET_DIR=/work/device-build/cargo-target
export PKG_CONFIG_ALLOW_CROSS=1
export PKG_CONFIG_LIBDIR=/work/device-build/sdl/lib/pkgconfig
export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_RUSTFLAGS='-C target-cpu=cortex-a53 -C link-arg=-Wl,-rpath,$ORIGIN/lib:/opt/vc/lib'
cargo build --locked --release --manifest-path ui/Cargo.toml --target armv7-unknown-linux-gnueabihf
cp device-build/cargo-target/armv7-unknown-linux-gnueabihf/release/gr1-ui device-build/package/gr1-ui
cp -L device-build/sdl/lib/libSDL2-2.0.so.0 device-build/package/lib/
