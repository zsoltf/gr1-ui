#!/bin/sh
# Apply only after the operator accepts this exact package and boot preview.
# Run as root from a verified package staged under gr1-ui-installed/.
set -eu
package_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$package_dir/stock-service.sh"
override=/etc/systemd/system/rungr.service.d/90-rust-ui.conf
stock=/home/sysop/GR-1/grui/grmain.exe
stock_hash=4b6ac24bff75f6b3e9d44a72965f1726e7936225d824572dba4f04a7ff5bf299
[ "$(id -u)" = 0 ] || { echo 'Run this approved install as root'; exit 1; }
[ "$(uname -s)" = Linux ] && [ "$(uname -m)" = armv7l ] || exit 1
grep -q 'Raspberry Pi 3 Model B Plus Rev 1.3' /proc/device-tree/model || exit 1
case "$package_dir" in
    /media/internal/gr1-ui-installed/*) ;;
    *) echo 'Use a versioned package under /media/internal/gr1-ui-installed'; exit 1 ;;
esac
basename "$package_dir" | grep -Eq '^[0-9a-f]{40}$' || {
    echo 'Package directory must be its full source commit'; exit 1;
}
[ "$(sha256sum "$stock" | cut -d ' ' -f1)" = "$stock_hash" ] || exit 1
gr1_check_stock_startup
[ ! -e "$override" ] || exit 1
if pgrep '^gr1-ui' >/dev/null || pgrep '^grmain_' >/dev/null; then
    echo 'Another engine host is running; finish its trial first'; exit 1;
fi
[ -f /media/internal/performance01/performance.grm ] || exit 1
[ ! -d /media/internal/user/perf ] || {
    echo 'Ambiguous data layout: inspect before installing'; exit 1;
}
cd "$package_dir"
sha256sum -c SHA256SUMS

changed=0
restore_on_error() {
    status=$?
    trap - EXIT
    if [ "$status" -ne 0 ]; then
        systemctl stop rungr.service || true
        if [ "$changed" = 1 ]; then
            rm -f "$override"
            systemctl daemon-reload
        fi
        gr1_restore_stock || exit 1
        echo 'Install failed; stock startup restored.'
    fi
    exit "$status"
}
trap restore_on_error EXIT
trap 'exit 130' HUP INT TERM
systemctl stop rungr.service
if pgrep -x grmain.exe >/dev/null; then
    echo 'Stock engine still running'; exit 1;
fi
./run-device.sh --check-device
mkdir -p /media/internal/gr1-ui-recovery
backup=/media/internal/gr1-ui-recovery/pre-install-$(date -u +%Y%m%dT%H%M%SZ).tar.gz
[ ! -e "$backup" ] || exit 1
set -- home/sysop/GR-1 root/rungr.new etc/systemd/system/rungr.service boot media/internal/factory
for index in $(seq -w 1 50); do
    set -- "$@" "media/internal/performance$index"
done
tar -C / -czf "$backup" "$@"
tar -tzf "$backup" >/dev/null
sha256sum "$backup"
mkdir -p /media/internal/gr1-ui-settings
if [ ! -e /media/internal/gr1-ui-settings/appearance.json ] &&
   [ -f /media/internal/gr1-ui-repair-data/ui-appearance.json ]; then
    cp /media/internal/gr1-ui-repair-data/ui-appearance.json /media/internal/gr1-ui-settings/appearance.json
fi
mkdir -p /etc/systemd/system/rungr.service.d
changed=1
cat > "$override" <<EOF
# GR1_UI_MANAGED_BOOT_V1
# Original service and stock application retained.
[Service]
Type=simple
RemainAfterExit=no
User=root
WorkingDirectory=$package_dir
ExecStartPre=$package_dir/prepare-host.sh
ExecStart=
ExecStart=$package_dir/run-device.sh --data-root /media/internal
KillMode=control-group
Restart=no
TimeoutStopSec=10
EOF
systemctl daemon-reload
systemctl --no-block start rungr.service
sleep 5
pid=$(systemctl show rungr.service -p MainPID | cut -d= -f2)
[ "$pid" -gt 0 ] && [ "$(readlink "/proc/$pid/exe")" = "$package_dir/gr1-ui" ] || {
    echo 'Installed executable is not running'; exit 1;
}
grep -q '^state: RUNNING' /proc/asound/card0/pcm0p/sub0/status || {
    echo 'Audio stream is not running'; exit 1;
}
trap - EXIT HUP INT TERM
echo "Installed $package_dir; PID $pid; ALSA running."
echo 'Check the physical screen, controls and sound. Cold boot remains unverified.'
echo "Stock recovery: sudo $package_dir/recover-stock.sh"
