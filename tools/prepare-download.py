#!/usr/bin/env python3
"""Add the vendor engine privately, reading firmware as data without execution."""
import argparse
import hashlib
import importlib.util
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import urllib.request
import venv

ROOT = Path(__file__).absolute().parent.parent
spec = importlib.util.spec_from_file_location('ci_bundle', Path(__file__).with_name('ci-bundle.py'))
bundle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bundle)
spec = importlib.util.spec_from_file_location('prepare_core', Path(__file__).with_name('prepare-core.py'))
core = importlib.util.module_from_spec(spec)
spec.loader.exec_module(core)
NAME = 'gr1_firmware_3.4.4_24117ba.tgz'
URL = 'https://www.tastychips.nl/downloads/GR-1/Firmware/' + NAME


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bundle', type=Path, required=True)
    parser.add_argument('--firmware', type=Path, help='existing unmodified official archive')
    parser.add_argument('--output', type=Path, default=ROOT / 'device-build')
    args = parser.parse_args()
    payload, revision = bundle.read_download(args.bundle)
    output = args.output.absolute()
    if output.resolve().is_relative_to(ROOT.resolve()) and output.resolve() != (ROOT / 'device-build').resolve():
        raise ValueError('private output inside this tree must use ignored device-build/')
    package, archive = output / 'package', output / 'gr1-ui-3.4.4-armhf.tar.gz'
    if package.exists() or package.is_symlink() or archive.exists() or archive.is_symlink():
        raise ValueError('private package already exists; use a new output directory')
    output.mkdir(parents=True, exist_ok=True)
    firmware = args.firmware
    if firmware is None:
        firmware = output / NAME
        if not firmware.exists():
            with tempfile.TemporaryDirectory(dir=output) as scratch:
                partial = Path(scratch) / NAME
                with urllib.request.urlopen(URL, timeout=60) as source, partial.open('wb') as dest:
                    shutil.copyfileobj(source, dest)
                core.checked_firmware(partial)
                partial.rename(firmware)
    core.checked_firmware(firmware)
    environment = output / 'tools-venv'
    venv.EnvBuilder(with_pip=True).create(environment)
    python = environment / 'bin/python'
    subprocess.run([str(python), '-m', 'pip', 'install', '-r',
                    str(ROOT / 'tools/owner-requirements.txt')], check=True)
    with tempfile.TemporaryDirectory(prefix='gr1-private-', dir=output) as scratch:
        subprocess.run([str(python), str(ROOT / 'tools/prepare-core.py'),
                        '--bundle', str(args.bundle.absolute()),
                        '--firmware', str(firmware.absolute()), '--output', scratch], check=True)
        (Path(scratch) / 'gr1-ui-3.4.4-armhf.tar.gz').rename(archive)
        (Path(scratch) / 'package').rename(package)
    print(f'Private package: {archive}; SHA256 {bundle.digest(archive.read_bytes())}')
    print('No engine executed or device changed. Keep this package and firmware private.')


if __name__ == '__main__':
    main()
