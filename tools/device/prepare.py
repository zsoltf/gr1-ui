#!/usr/bin/env python3
"""Extract a private, pinned build sysroot and engine without executing either."""
import argparse
import hashlib
import os
from pathlib import Path
import shutil
import stat
import sys
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from inspect_native import decode_native, NATIVE_SHA256
from dissect.volume.disk import Disk
from dissect.extfs import ExtFS

IMAGE_ZIP_SHA = 'e0138f632613cb9bec93c8570692067c444e1fd28fc78fe57faec8712366991c'
IMAGE_SHA = 'ba49cc3be7b12d40efadcf2b2a8dd929210dedfc47cddb801c632e67005c1c98'
IMAGE_MEMBER = 'gr1_3part_3.0.0-b9_15jan25.img'


def digest(path):
    with path.open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--image', type=Path, required=True)
    p.add_argument('--firmware', type=Path, required=True)
    p.add_argument('--out', type=Path, default=Path('device-build'))
    a = p.parse_args()
    a.out.mkdir(parents=True, exist_ok=True)
    if digest(a.image) != IMAGE_ZIP_SHA:
        raise ValueError('wrong official system image')
    image = a.out / IMAGE_MEMBER
    if not image.exists():
        with zipfile.ZipFile(a.image) as z, z.open(IMAGE_MEMBER) as src, image.open('wb') as dst:
            shutil.copyfileobj(src, dst, 1024 * 1024)
    if digest(image) != IMAGE_SHA:
        raise ValueError('wrong raw system image')
    root = (a.out / 'sysroot').resolve()
    root.mkdir(exist_ok=True)
    with image.open('rb') as f:
        fs = ExtFS(Disk(f).partitions[1].open())
        def extract(path):
            node = fs.get(path)
            dest = root / path.lstrip('/')
            if node.filetype == stat.S_IFLNK:
                target = node.link
                if isinstance(target, bytes):
                    target = target.decode()
                if target.startswith('/'):
                    target = os.path.relpath(root / target.lstrip('/'), dest.parent)
                dest.parent.mkdir(parents=True, exist_ok=True)
                if dest.is_symlink():
                    dest.unlink()
                if not dest.exists():
                    dest.symlink_to(target)
            elif node.filetype == stat.S_IFDIR:
                dest.mkdir(parents=True, exist_ok=True)
                for entry in node.iterdir():
                    if entry.filename not in ('.', '..'):
                        extract(path.rstrip('/') + '/' + entry.filename)
            elif node.filetype == stat.S_IFREG:
                dest.parent.mkdir(parents=True, exist_ok=True)
                with node.open() as src, dest.open('wb') as dst:
                    shutil.copyfileobj(src, dst)
            else:
                raise ValueError(f'unsupported sysroot entry {path}')
        for path in ('/lib', '/usr/lib', '/usr/include', '/opt/vc',
                     '/usr/share/doc/fonts-dejavu-core/copyright',
                     '/usr/share/fonts/truetype/dejavu', '/home/sysop/.internal'):
            print('Extracting', path, flush=True)
            extract(path)
    lib = a.out / 'package/lib/libgr.so.1.0.1'
    lib.parent.mkdir(parents=True, exist_ok=True)
    lib.write_bytes(decode_native(a.firmware))
    assert digest(lib) == NATIVE_SHA256
    print('Private sysroot, stock fixtures and original engine prepared; no vendor code executed.')

if __name__ == '__main__':
    main()
