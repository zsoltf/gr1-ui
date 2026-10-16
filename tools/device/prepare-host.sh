#!/bin/sh
# Preserve the stock launcher's transient setup on this GR-1 before startup.
set -eu
[ "$(id -u)" = 0 ] || { echo 'GR-1 host setup requires root'; exit 1; }
[ "$(uname -s)" = Linux ] && [ "$(uname -m)" = armv7l ] || exit 1
grep -q 'Raspberry Pi 3 Model B Plus Rev 1.3' /proc/device-tree/model || exit 1

if [ -e /proc/asound/card0/pcm0p/sub0/prealloc ]; then
    echo 4096 > /proc/asound/card0/pcm0p/sub0/prealloc
fi
for cpu in 0 1 2 3; do
    echo performance > "/sys/devices/system/cpu/cpu$cpu/cpufreq/scaling_governor"
done
/bin/setserial /dev/ttyAMA0 low_latency
