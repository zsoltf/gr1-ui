#!/bin/sh
# Read metadata on the running GR-1. Never source its scripts or load its engine.
# Intended invocation from the Mac: ssh user@host 'sh -s' < this-file
set -u
export LC_ALL=C

case "${1:-}" in
    --help|-h)
        printf '%s\n' 'Usage: sh tools/inspect_device.sh' \
            'Run on the Linux instrument, directly or through SSH stdin.' \
            'Outputs metadata only; makes no device changes.'
        exit 0
        ;;
    '') ;;
    *) printf '%s\n' 'No target arguments are accepted. Run --help.' >&2; exit 2 ;;
esac

if [ "$(uname -s)" != Linux ]; then
    printf '%s\n' 'Run this probe on the Linux GR-1, not on the development Mac.' >&2
    exit 2
fi

gr1_run() {
    printf '\n[%s]\n' "$1"
    shift
    if ! command -v "$1" >/dev/null 2>&1; then
        printf 'unavailable command: %s\n' "$1"
        return
    fi
    if command -v timeout >/dev/null 2>&1; then
        timeout 10 "$@"
    else
        "$@"
    fi
    gr1_rc=$?
    if [ "$gr1_rc" -ne 0 ]; then
        printf 'inspection incomplete: exit %s\n' "$gr1_rc"
    fi
    return 0
}

gr1_read() {
    if [ -f "$1" ] && [ -r "$1" ]; then
        # Only explicitly selected metadata paths; never FIFOs or devices.
        gr1_run "$1 (first 16384 bytes)" head -c 16384 "$1" | tr '\000' '\n'
        printf '\n'
    else
        printf '\n[%s]\nmissing, unreadable, or not a regular file\n' "$1"
    fi
}

gr1_hash() {
    if [ -f "$1" ] && [ -r "$1" ]; then
        gr1_run "$1 SHA-256" sha256sum "$1"
    else
        printf '\n[%s]\nmissing, unreadable, or not a regular file\n' "$1"
    fi
}

printf '%s\n' 'GR-1 runtime inventory v1' \
    'Read-only metadata. This report is not a hardware or compatibility pass.' \
    'No engine code, service changes, package installation or device writes.'
gr1_run 'UTC observation time' date -u '+%Y-%m-%dT%H:%M:%SZ'
gr1_run 'Kernel and machine' uname -srm
gr1_run 'Userspace word size' getconf LONG_BIT
gr1_run 'C library' getconf GNU_LIBC_VERSION
gr1_run 'Debian architecture' dpkg --print-architecture
gr1_read /etc/os-release
gr1_read /proc/device-tree/model
gr1_run 'Probe identity and groups' id

gr1_run 'Stock service metadata' systemctl show rungr.service --no-pager \
    -p LoadState -p FragmentPath -p DropInPaths -p ActiveState -p SubState \
    -p User -p ExecStart -p MainPID -p ControlGroup -p KillMode -p Restart
# Do not collect process arguments or environment: they can contain credentials.
gr1_run 'Process names and parent IDs' ps -eo pid,ppid,user,comm
for gr1_path in \
    /etc/systemd/system/rungr.service \
    /root/rungr.new \
    /home/sysop/GR-1/grui/run.sh \
    /home/sysop/GR-1/grui/grmain_nologging.cpython-34.pyo \
    /home/sysop/GR-1/grui/graphics \
    /tmp/graphics \
    /boot/config.txt \
    /boot/config.newos.txt
do
    gr1_hash "$gr1_path"
done

gr1_read /proc/fb
for gr1_path in /sys/class/graphics/fb*/name \
    /sys/class/graphics/fb*/virtual_size /sys/class/graphics/fb*/bits_per_pixel
do
    gr1_read "$gr1_path"
done
gr1_read /proc/bus/input/devices
gr1_read /proc/asound/cards
gr1_read /proc/asound/pcm
gr1_run 'Audio device nodes' ls -l /dev/snd
gr1_run 'Input device nodes' ls -l /dev/input
gr1_run 'DRM device nodes' ls -l /dev/dri
gr1_run 'Panel UART metadata (not opened)' ls -l /dev/ttyAMA0

printf '\n[Relevant shared-library cache entries]\n'
if command -v ldconfig >/dev/null 2>&1; then
    # Query the cache only; never run ldd on or execute the proprietary engine.
    gr1_libs=$(ldconfig -p 2>&1)
    gr1_rc=$?
    if [ "$gr1_rc" -eq 0 ]; then
        printf '%s\n' "$gr1_libs" | awk \
            '/lib(SDL2|asound|sndfile|stdc\+\+|gcc_s|pthread|bcm_host|EGL|GLES|drm|gbm)|lib[cm]\./ {print; n++} END {if (!n) print "no matching cache entries"}'
    else
        printf 'cache inspection failed: exit %s\n' "$gr1_rc"
    fi
else
    printf '%s\n' 'ldconfig unavailable in PATH; library availability unobserved'
fi

gr1_run 'Block devices and mountpoints' lsblk -o NAME,TYPE,SIZE,FSTYPE,MOUNTPOINT
for gr1_path in / /boot /media/internal; do
    if [ -d "$gr1_path" ]; then
        gr1_run "$gr1_path filesystem" findmnt -n -T "$gr1_path" \
            -o TARGET,SOURCE,FSTYPE,OPTIONS
        gr1_run "$gr1_path free space" df -Pk "$gr1_path"
    else
        printf '\n[%s]\nmissing directory\n' "$gr1_path"
    fi
done
printf '\n%s\n' 'Inventory finished; missing sections remain unobserved.' \
    'Keep this report local. No audio, panel, display or recovery was exercised.'
