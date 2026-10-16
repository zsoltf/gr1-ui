#!/usr/bin/env python3
"""Privately regenerate build metadata from pinned firmware, never executing it."""
import argparse
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import urllib.request
import venv

ROOT = Path(__file__).resolve().parent.parent
SHA256 = '559e1e64bca5b16e17567231aeeb82eae4640adab9c06d910da15275294f4c5b'
NAME = 'gr1_firmware_3.4.4_24117ba.tgz'
URL = 'https://www.tastychips.nl/downloads/GR-1/Firmware/' + NAME


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--firmware', type=Path,
                        help='existing unmodified official archive (or GR1_FIRMWARE)')
    parser.add_argument('--python', type=Path,
                        help='existing Python with pinned tools/device/requirements.txt')
    args = parser.parse_args()
    downloads = ROOT / 'device-build/downloads'
    archive = args.firmware or (Path(os.environ['GR1_FIRMWARE'])
                               if 'GR1_FIRMWARE' in os.environ else downloads / NAME)
    if not archive.is_file():
        if args.firmware or 'GR1_FIRMWARE' in os.environ:
            raise FileNotFoundError(archive)
        downloads.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=downloads) as scratch:
            partial = Path(scratch) / NAME
            with urllib.request.urlopen(URL, timeout=60) as source, partial.open('wb') as dest:
                shutil.copyfileobj(source, dest)
            with partial.open('rb') as source:
                if hashlib.file_digest(source, 'sha256').hexdigest() != SHA256:
                    raise ValueError('download does not match pinned firmware')
            partial.rename(archive)
    with archive.open('rb') as source:
        if hashlib.file_digest(source, 'sha256').hexdigest() != SHA256:
            raise ValueError('archive does not match pinned firmware')
    python = args.python
    if python is None:
        environment = ROOT / 'device-build/tools-venv'
        venv.EnvBuilder(with_pip=True).create(environment)
        python = environment / 'bin/python'
        subprocess.run([str(python), '-m', 'pip', 'install', '-r',
                        str(ROOT / 'tools/device/requirements.txt')], check=True)
    destination = ROOT / 'ui/src/device'
    with tempfile.TemporaryDirectory(prefix='gr1-metadata-') as scratch:
        subprocess.run([str(python.absolute()), str(ROOT / 'tools/device/generate.py'),
                        str(archive.resolve()), '--out', scratch], cwd=ROOT, check=True)
        for name in ('abi.rs', 'defaults.json'):
            shutil.copyfile(Path(scratch) / name, destination / name)
    print('Pinned build tables regenerated locally; keep firmware and tables private.')


if __name__ == '__main__':
    main()
