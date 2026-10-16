#!/bin/sh
# Approved isolated trial; changes no boot configuration or stock data.
set -eu
package_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$package_dir/stock-service.sh"
data=${1:?Supply the isolated trial data root}
prepare=${2:-}
case "$data" in
    /media/internal/gr1-ui-repair-data|/media/internal/gr1-ui-repair-data/*) ;;
    *) echo 'Use the approved isolated repair data tree'; exit 1 ;;
esac
[ "$(id -u)" = 0 ] || { echo 'Run the trial as root'; exit 1; }
if [ "$prepare" = --copy-stock ]; then
    [ ! -e "$data" ] || { echo 'Refusing to overwrite trial data'; exit 1; }
elif [ -n "$prepare" ]; then
    echo 'Unknown trial option'; exit 1
else
    [ -f "$data/performance01/performance.grm" ] ||
        [ -f "$data/user/perf/performance01/performance.grm" ] || exit 1
fi
cd "$package_dir"
sha256sum -c SHA256SUMS
gr1_check_stock_startup
if pgrep '^gr1-ui' >/dev/null || pgrep '^grmain_' >/dev/null; then
    echo 'Another engine host is running; finish its trial first'; exit 1;
fi
child=
tailpid=
cleanup() {
    status=$?
    trap - EXIT HUP INT TERM
    if [ -n "$child" ] && kill -0 "$child" 2>/dev/null; then
        kill -TERM "$child" 2>/dev/null || true
        for attempt in $(seq 1 20); do
            kill -0 "$child" 2>/dev/null || break
            sleep 0.25
        done
        if kill -0 "$child" 2>/dev/null; then
            kill -KILL "$child" 2>/dev/null || true
        fi
        wait "$child" 2>/dev/null || true
    fi
    [ -z "$tailpid" ] || kill "$tailpid" 2>/dev/null || true
    gr1_restore_stock || exit 1
    echo 'Trial ended.'
    exit "$status"
}
trap cleanup EXIT
trap 'exit 130' HUP INT TERM
systemctl stop rungr.service
./run-device.sh --check-device
if [ "$prepare" = --copy-stock ]; then
    echo "Copying stock data into isolated $data"
    mkdir "$data"
    cp -R /media/internal/performance?? /media/internal/factory "$data/"
    mkdir "$data/gr1-ui-settings"
    if [ -f /media/internal/gr1-ui-repair-data/ui-appearance.json ]; then
        cp /media/internal/gr1-ui-repair-data/ui-appearance.json "$data/gr1-ui-settings/appearance.json"
    fi
fi
log="$package_dir/trial-$(date -u +%Y%m%dT%H%M%SZ).log"
echo "Trial log: $log; Ctrl-C returns to stock."
GR1_UI_TRACE=1 GR1_UI_PANEL_TRACE=1 ./run-device.sh --data-root "$data" >"$log" 2>&1 &
child=$!
tail -n +1 -f "$log" &
tailpid=$!
wait "$child"
child=
