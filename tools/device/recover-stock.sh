#!/bin/sh
# Remove only this project's boot override; keep package, stock and data.
set -eu
package_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$package_dir/stock-service.sh"
override=/etc/systemd/system/rungr.service.d/90-rust-ui.conf
[ "$(id -u)" = 0 ] || { echo 'Run stock recovery as root'; exit 1; }
[ "$(uname -s)" = Linux ] || exit 1
if [ -e "$override" ]; then
    grep -q '^# GR1_UI_MANAGED_BOOT_V1$' "$override" || {
        echo 'Unrecognized override: inspect it before removal'; exit 1;
    }
    systemctl stop rungr.service
    rm "$override"
    systemctl daemon-reload
fi
gr1_restore_stock
