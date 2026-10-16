#!/usr/bin/env python3
"""Package only redistributable UI output; never load or execute an engine."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import tarfile

ROOT = Path(__file__).absolute().parent.parent
DOWNLOAD_FILES = (
    'gr1-ui', 'lib/libSDL2-2.0.so.0', 'fonts/DejaVuSans.ttf',
    'fonts/DejaVuSans-Bold.ttf', 'fonts/DejaVuSansCondensed.ttf',
    'FONT-LICENSE.txt', 'FONT-LICENSE-DejaVu-2.37.txt', 'SDL-LICENSE.txt',
    'LICENSE', 'COPYRIGHT', 'THIRD-PARTY-NOTICES.md', 'DEPENDENCY-LICENSES.txt',
    'README.md', '.gitignore', 'SOURCE_REVISION', 'ui/README.md',
    'ui/Cargo.lock', 'ui/fonts/LICENSE-DejaVu.txt', 'tools/device/requirements.txt',
    'docs/owner-install.md', 'docs/device-build.md',
    'web/index.html', 'web/app.js', 'web/worker.js',
    'web/images/main.jpg', 'web/images/sample-record.jpg',
    'web/images/patch-matrix.jpg', 'web/images/patch-lfo.jpg',
    'web/images/performance-parts.jpg', 'web/images/single-part.jpg',
    'run-device.sh', 'prepare-host.sh', '90-rust-ui.conf', 'stock-service.sh',
    'temporary-trial.sh', 'install-boot.sh', 'recover-stock.sh', 'inspect-device.sh',
    'tools/ci-bundle.py', 'tools/prepare-download.py', 'tools/prepare-core.py', 'tools/inspect_native.py',
    'tools/inspect_firmware.py', 'tools/owner-requirements.txt',
)
ENGINE = 'lib/libgr.so.1.0.1'


def digest(data):
    return hashlib.sha256(data).hexdigest()


def metadata(files, revision, private=False):
    if not re.fullmatch('[0-9a-f]{40}', revision):
        raise ValueError('full source revision required')
    manifest = {'target': 'armv7-unknown-linux-gnueabihf', 'firmware': '3.4.4',
                'source_base': revision, 'source_has_local_changes': None,
                'contains_vendor_engine': private,
                'files_sha256': {name: digest(data) for name, (data, _) in sorted(files.items())}}
    data = (json.dumps(manifest, indent=2) + '\n').encode()
    files = dict(files, **{'manifest.json': (data, 0o644)})
    sums = ''.join(f'{digest(data)}  {name}\n' for name, (data, _) in sorted(files.items()))
    files['SHA256SUMS'] = (sums.encode(), 0o644)
    return files


def write_archive(path, files, prefix):
    with path.open('xb') as destination, tarfile.open(fileobj=destination, mode='w:gz') as archive:
        for name, (data, mode) in sorted(files.items()):
            member = tarfile.TarInfo(prefix + '/' + name)
            member.size, member.mode = len(data), mode
            archive.addfile(member, io.BytesIO(data))


def read_download(path):
    expected = set(DOWNLOAD_FILES) | {'manifest.json', 'SHA256SUMS'}
    files = {}
    with tarfile.open(path) as archive:
        for member in archive.getmembers():
            name = member.name.removeprefix('gr1-ui-download/')
            if member.name != 'gr1-ui-download/' + name or name not in expected or name in files or not member.isfile():
                raise ValueError(f'unexpected download entry: {member.name}')
            files[name] = (archive.extractfile(member).read(), member.mode)
    if set(files) != expected:
        raise ValueError('download is missing required files')
    manifest = json.loads(files['manifest.json'][0])
    if manifest.get('contains_vendor_engine') is not False:
        raise ValueError('download must not contain a vendor engine')
    payload = {name: files[name] for name in DOWNLOAD_FILES}
    expected_metadata = metadata(payload, manifest['source_base'])
    for name in ('manifest.json', 'SHA256SUMS'):
        if files[name][0] != expected_metadata[name][0]:
            raise ValueError('download hashes or metadata do not match')
    return payload, manifest['source_base']


def dependency_notices(path):
    packages = json.loads(path.read_text())['packages']
    sdl_wrapper = next(Path(p['manifest_path']).parent for p in packages if p['name'] == 'sdl2')
    notices = []
    allowed = {'LICENSE', 'LICENSE.txt', 'LICENSE.md', 'COPYING', 'COPYRIGHT', 'NOTICE',
               'LICENSE-APACHE', 'LICENSE-MIT', 'LICENSE-ZLIB', 'LICENSE-0BSD',
               'LICENSE-UNICODE', 'LICENSE-MIT.md', 'LICENSE-ZLIB.md', 'LICENSE-APACHE.md',
               'license-apache-2.0', 'license-mit'}
    for package in sorted(packages, key=lambda p: (p['name'], p['version'])):
        if package['name'] == 'gr1-ui':
            continue
        root = Path(package['manifest_path']).parent
        licenses = [root / name for name in sorted(allowed) if (root / name).is_file()]
        if package['name'] == 'sdl2-sys':
            licenses += [sdl_wrapper / 'LICENSE', root / 'SDL/LICENSE.txt']
        if not licenses:
            raise ValueError('missing dependency notices: ' + package['name'])
        notices.append(f"\n{package['name']} {package['version']}: {package['license']}\n")
        for license_file in licenses:
            notices.append(f'\n{license_file.name}\n{license_file.read_text()}\n')
    return ''.join(notices).encode()


def bundle(package, assets, source, cargo_metadata, revision, output):
    files = {}
    for name in DOWNLOAD_FILES:
        if name == 'DEPENDENCY-LICENSES.txt':
            files[name] = (dependency_notices(cargo_metadata), 0o644)
            continue
        if name in ('gr1-ui', 'lib/libSDL2-2.0.so.0'):
            path = package / name
        elif name.startswith('fonts/') or name in ('FONT-LICENSE.txt', 'FONT-LICENSE-DejaVu-2.37.txt', 'SDL-LICENSE.txt'):
            path = assets / name
        elif name in ('run-device.sh', 'prepare-host.sh', '90-rust-ui.conf', 'stock-service.sh',
                      'temporary-trial.sh', 'install-boot.sh', 'recover-stock.sh'):
            path = source / 'tools/device' / name
        elif name == 'inspect-device.sh':
            path = source / 'tools/inspect_device.sh'
        else:
            path = source / name
        if path.is_symlink() or not path.is_file():
            raise ValueError(f'required regular file missing: {path}')
        files[name] = (path.read_bytes(), 0o755 if name == 'gr1-ui' or name.endswith(('.sh', '.py')) else 0o644)
    # Identity of the build, including Actions checkouts, supersedes export base.
    files['SOURCE_REVISION'] = ((revision + '\n').encode(), 0o644)
    files = metadata(files, revision)
    write_archive(output, files, 'gr1-ui-download')
    read_download(output)
    print(f'UI-only download: {output}; {len(files)} files; SHA256 {digest(output.read_bytes())}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--package', type=Path, default=ROOT / 'device-build/package')
    parser.add_argument('--assets', type=Path, help='licensed assets from a complete private package')
    parser.add_argument('--source', type=Path, default=ROOT)
    parser.add_argument('--cargo-metadata', type=Path, default=ROOT / 'device-build/cargo-metadata.json')
    parser.add_argument('--revision', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    bundle(args.package, args.assets or args.package, args.source,
           args.cargo_metadata, args.revision, args.output)


if __name__ == '__main__':
    main()
