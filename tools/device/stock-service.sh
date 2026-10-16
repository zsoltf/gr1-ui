#!/bin/sh
# Shared stock startup identity and recovery checks. Sourcing makes no changes.
gr1_check_stock_startup() {
    gr1_dropins=$(systemctl show rungr.service -p DropInPaths) || {
        echo 'Cannot query stock startup; no service change made'
        return 1
    }
    case "$gr1_dropins" in
        DropInPaths=*) ;;
        *) echo 'Stock startup query is incomplete'; return 1 ;;
    esac
    if [ -n "${gr1_dropins#DropInPaths=}" ]; then
        echo 'Stock startup has an override; temporary stock recovery is unavailable'
        return 1
    fi
    gr1_hash_output=$(sha256sum /etc/systemd/system/rungr.service) || {
        echo 'Cannot read stock service hash; no service change made'
        return 1
    }
    gr1_service_hash=${gr1_hash_output%% *}
    if [ "$gr1_service_hash" != 96350a1360b3df90d8c19318b1798bda954d675405ec347a673774f9712ee12f ]; then
        echo 'Stock service changed; inspect startup before running'
        return 1
    fi
    return 0
}

gr1_restore_stock() {
    if pgrep '^gr1-ui' >/dev/null || pgrep '^grmain_' >/dev/null; then
        echo 'Another engine host remains; stock restart withheld'
        return 1
    fi
    gr1_check_stock_startup || return 1
    systemctl --no-block start rungr.service || return 1
    echo 'Waiting up to 30 seconds for the stock process and audio stream.'
    for gr1_attempt in $(seq 0 30); do
        if pgrep -x grmain.exe >/dev/null &&
            head -1 /proc/asound/card0/pcm0p/sub0/status 2>/dev/null | grep -q '^state: RUNNING'; then
            echo 'Stock process and audio stream returned; check the physical screen and sound.'
            return 0
        fi
        [ "$gr1_attempt" -eq 30 ] || sleep 1
    done
    if ! pgrep -x grmain.exe >/dev/null; then
        echo 'Stock process did not return within 30 seconds; recovery failed'
        return 1
    fi
    if ! head -1 /proc/asound/card0/pcm0p/sub0/status | grep -q '^state: RUNNING'; then
        echo 'Stock audio stream did not return within 30 seconds; recovery failed'
        return 1
    fi
    echo 'Stock readiness changed at the deadline; recovery failed'
    return 1
}
