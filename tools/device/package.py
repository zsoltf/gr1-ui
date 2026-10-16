#!/usr/bin/env python3
"""Package private local output. No device writes or vendor execution."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tarfile


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--root', type=Path, default=Path('device-build'))
    p.add_argument('--sdl-source', type=Path, required=True)
    a = p.parse_args()
    package = a.root / 'package'
    for path in ('gr1-ui', 'lib/libSDL2-2.0.so.0', 'lib/libgr.so.1.0.1'):
        if not (package / path).is_file():
            raise FileNotFoundError(package / path)
    for name in ('run-device.sh', 'prepare-host.sh', '90-rust-ui.conf', 'stock-service.sh', 'temporary-trial.sh', 'install-boot.sh', 'recover-stock.sh'):
        shutil.copyfile(Path('tools/device') / name, package / name)
        if name.endswith('.sh'):
            (package / name).chmod(0o755)
    (package / 'run-device.sh').chmod(0o755)
    (package / 'gr1-ui').chmod(0o755)
    (package / 'fonts').mkdir(exist_ok=True)
    shutil.copyfile(a.root / 'sysroot/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf', package / 'fonts/DejaVuSans.ttf')
    shutil.copyfile(a.sdl_source / 'LICENSE.txt', package / 'SDL-LICENSE.txt')
    # Include the actual font package copyright from the official image.
    font_license = a.root / 'sysroot/usr/share/doc/fonts-dejavu-core/copyright'
    if not font_license.is_file():
        raise FileNotFoundError('Prepare the font copyright file from the official image')
    shutil.copyfile(font_license, package / 'FONT-LICENSE.txt')
    # The two added DejaVu 2.37 faces sit beside the regular face, with their license.
    for name in ('DejaVuSans-Bold.ttf', 'DejaVuSansCondensed.ttf'):
        shutil.copyfile(Path('ui/fonts') / name, package / 'fonts' / name)
    shutil.copyfile('ui/fonts/LICENSE-DejaVu.txt', package / 'FONT-LICENSE-DejaVu-2.37.txt')
    shutil.copyfile('tools/inspect_device.sh', package / 'inspect-device.sh')
    (package / 'README.txt').write_text('GR-1 Rust UI - private local ARM32 package, engine 3.4.4\n\nThis output includes proprietary vendor engine and image-derived libraries.\nKeep it private. Source and build instructions are in README.md and\ndocs/device-build.md at SOURCE_REVISION. Packaging does not establish sound,\npanel behavior, timing or installation acceptance. Never execute this package\non a Mac, generic Linux, a generic Pi or an emulator: its audio failure path\ncan reboot the host.\n\nRead docs/device-build.md before any trial or installation. Back up the exact\nunit, inspect original startup, verify SHA256SUMS and stop the identified stock\napplication and every engine child. Never start competing engine hosts.\ntemporary-trial.sh supports an isolated copied data root and restores stock on\nchild exit. Identify and verify the exact engine PID/executable independently,\nthen TERM that engine to stop; independently check stock process, ALSA RUNNING,\nscreen and sound. Wrapper signals through sudo/SSH remain unverified on device.\n\ninstall-boot.sh and recover-stock.sh retain exact host, stock and service guards.\nThese earlier network helpers target stock firmware 2.8.3. Use the browser\nUSB update for firmware 3.4.4. Verify the target and recovery procedure first.\nInstall preserves the original files, creates a recovery archive\nand writes one marked override. ExecStartPre runs prepare-host.sh for original\nALSA preallocation, four CPU governors and UART low latency; run-device.sh sets\nthe original locale. No partition expansion, firmware change or reboot occurs.\nrecover-stock.sh removes only its recognized override and restores stock.\nCold boot and physical recovery require observation on the actual instrument.\n\nFonts and SDL licenses accompany this private package. See public source\nTHIRD-PARTY-NOTICES.md for the source dependency inventory.\n')
    files = sorted(path for path in package.rglob('*') if path.is_file() and path.name not in ('manifest.json', 'SHA256SUMS'))
    hashes = {str(path.relative_to(package)): hashlib.sha256(path.read_bytes()).hexdigest() for path in files}
    if Path('.git').exists():
        source_revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
        source_dirty = bool(subprocess.check_output(['git', 'status', '--porcelain'], text=True).strip())
    else:
        source_revision = Path('SOURCE_REVISION').read_text().strip()
        source_dirty = None
    (package / 'manifest.json').write_text(json.dumps({
        'target': 'armv7-unknown-linux-gnueabihf', 'firmware': '3.4.4',
        'source_base': source_revision,
        'source_has_local_changes': source_dirty,
        'rust': '1.90.0', 'sysroot': 'official gr1_3part_3.0.0-b9_15jan25 image, glibc 2.19',
        'hardware_evidence': 'docs/device-build.md at source_base; observe the actual target', 'files_sha256': hashes,
    }, indent=2) + '\n')
    hashes['manifest.json'] = hashlib.sha256((package / 'manifest.json').read_bytes()).hexdigest()
    (package / 'SHA256SUMS').write_text(''.join(f'{value}  {name}\n' for name, value in sorted(hashes.items())))
    archive = a.root / 'gr1-ui-3.4.4-armhf.tar.gz'
    with tarfile.open(archive, 'w:gz') as tar:
        tar.add(package, arcname='gr1-ui')
    print(f'{archive}: {archive.stat().st_size} bytes, SHA256 {hashlib.sha256(archive.read_bytes()).hexdigest()}')


if __name__ == '__main__':
    main()
