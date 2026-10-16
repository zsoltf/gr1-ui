#!/usr/bin/env python3
"""Prepare a private stock-updater archive without executing vendor software."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile

spec = importlib.util.spec_from_file_location('prepare_core', Path(__file__).with_name('prepare-core.py'))
core = importlib.util.module_from_spec(spec)
spec.loader.exec_module(core)

RECOVERY_NAME = 'gr1-use-original.txt'
RECOVERY_TEXT = b'Keep this file in the root of a USB stick to start the original GR-1 interface.\nRemove it to start the redesigned interface again.\n'

# Stock's /root/rungr.new still performs the hardware setup and calls run.sh.
# Select exactly one host, before either host opens audio or the panel UART.
LAUNCHER = b'''#!/bin/sh
trap '' USR1 USR2
cd /home/sysop/GR-1/grui || exit 1
stock() { exec /bin/sh ./run-stock.sh; }
case "$(uname -m)" in armv7l) ;; *) stock ;; esac
grep -q 'Raspberry Pi 3 Model B Plus Rev 1.3' /proc/device-tree/model || stock
# USB mounts can arrive after the service starts. Allow a short boot window.
for attempt in 1 2 3 4 5; do
    for drive in /media/usb/.usb0 /media/usb/.usb1 /media/usb/.usb2 /media/usb/.usb3; do
        [ ! -f "$drive/gr1-use-original.txt" ] || stock
        for update in "$drive"/gr1_firmware_*.tgz; do
            [ ! -f "$update" ] || stock
        done
    done
    sleep 1
done
(cd gr1-ui && sha256sum -c SHA256SUMS >/dev/null 2>&1) || stock
exec ./gr1-ui/run-device.sh --data-root /media/internal
'''

RUNTIME = (
    'gr1-ui', 'run-device.sh', 'lib/libSDL2-2.0.so.0', 'lib/libgr.so.1.0.1',
    'fonts/DejaVuSans.ttf', 'fonts/DejaVuSans-Bold.ttf', 'fonts/DejaVuSansCondensed.ttf',
    'FONT-LICENSE.txt', 'FONT-LICENSE-DejaVu-2.37.txt', 'SDL-LICENSE.txt',
    'LICENSE', 'COPYRIGHT', 'THIRD-PARTY-NOTICES.md', 'DEPENDENCY-LICENSES.txt',
    'SOURCE_REVISION',
)


def prepare(download, firmware, output):
    payload = core.prepare_files(download, firmware)
    revision = payload['SOURCE_REVISION'][0].decode().strip()
    files = {}
    with tarfile.open(firmware) as archive:
        for member in archive:
            name = member.name
            if name.startswith('/') or any(p in ('', '.', '..') for p in name.split('/')):
                raise ValueError('Unexpected official archive path')
            if member.isdir():
                continue
            if not member.isfile() or name in files:
                raise ValueError('Unexpected official archive entry')
            files[name] = (archive.extractfile(member).read(), member.mode)
    if 'grui/run-stock.sh' in files or any(n.startswith('grui/gr1-ui/') for n in files):
        raise ValueError('Expected an unmodified official firmware archive')
    files['grui/run-stock.sh'] = files['grui/run.sh']
    files['grui/run.sh'] = (LAUNCHER, 0o755)
    runtime = {name: payload[name] for name in RUNTIME}
    sums = ''.join(f'{hashlib.sha256(data).hexdigest()}  {name}\n'
                   for name, (data, _) in sorted(runtime.items())).encode()
    runtime['SHA256SUMS'] = (sums, 0o644)
    files.update({'grui/gr1-ui/' + name: value for name, value in runtime.items()})
    files['grui/GR1_UI_USB.json'] = (json.dumps({
        'firmware': '3.4.4', 'source_revision': revision,
        'recovery_file': RECOVERY_NAME,
        'hardware_tested': False,
    }, indent=2).encode() + b'\n', 0o644)
    with output.open('xb') as destination, tarfile.open(fileobj=destination, mode='w:gz', format=tarfile.USTAR_FORMAT) as archive:
        for name, (data, mode) in sorted(files.items()):
            member = tarfile.TarInfo(name)
            member.size, member.mode = len(data), mode
            archive.addfile(member, io.BytesIO(data))
    return {
        'filename': f'gr1_firmware_3.4.4_ui_{revision[:7]}.tgz',
        'packageSha256': hashlib.sha256(output.read_bytes()).hexdigest(),
        'engineSha256': hashlib.sha256(payload[core.bundle.ENGINE][0]).hexdigest(),
        'sourceRevision': revision,
    }
