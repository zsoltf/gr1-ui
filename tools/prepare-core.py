#!/usr/bin/env python3
"""Shared static preparation for local Python and browser Python runtimes."""
import argparse
import hashlib
import importlib.util
from pathlib import Path

spec = importlib.util.spec_from_file_location('ci_bundle', Path(__file__).with_name('ci-bundle.py'))
bundle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bundle)
FIRMWARE_SHA256 = '559e1e64bca5b16e17567231aeeb82eae4640adab9c06d910da15275294f4c5b'


def checked_firmware(path):
    with path.open('rb') as source:
        if hashlib.file_digest(source, 'sha256').hexdigest() != FIRMWARE_SHA256:
            raise ValueError('official firmware hash does not match; preparation stopped')


def prepare_files(download, firmware):
    payload, revision = bundle.read_download(download)
    checked_firmware(firmware)
    from inspect_native import decode_native
    payload[bundle.ENGINE] = (decode_native(firmware), 0o644)
    return bundle.metadata(payload, revision, private=True)


def write_private(files, output):
    package, archive = output / 'package', output / 'gr1-ui-3.4.4-armhf.tar.gz'
    if package.exists() or package.is_symlink() or archive.exists() or archive.is_symlink():
        raise ValueError('private package already exists')
    package.mkdir(parents=True)
    for name, (data, mode) in files.items():
        target = package / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        target.chmod(mode)
    bundle.write_archive(archive, files, 'gr1-ui')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bundle', type=Path, required=True)
    parser.add_argument('--firmware', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    write_private(prepare_files(args.bundle, args.firmware), args.output)


if __name__ == '__main__':
    main()
