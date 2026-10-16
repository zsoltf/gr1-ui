#!/bin/sh
# Foreground host only: does not install, stop stock, or change boot settings.
set -eu
package_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$package_dir"
export LANG=en_US.UTF-8
export LC_ALL=en_US.UTF-8
export LD_LIBRARY_PATH="$package_dir/lib:/opt/vc/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export SDL_VIDEODRIVER=rpi
export SDL_AUDIODRIVER=dummy
export GR1_UI_FONT="$package_dir/fonts/DejaVuSans.ttf"
exec "$package_dir/gr1-ui" --device --engine "$package_dir/lib/libgr.so.1.0.1" "$@"
