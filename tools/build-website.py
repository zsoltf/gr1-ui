#!/usr/bin/env python3
"""Build the owner website with its redistributable UI download only."""
import argparse
import importlib.util
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parent.parent
TOOLS = ('ci-bundle.py', 'prepare-core.py', 'usb-update.py', 'inspect_native.py', 'inspect_firmware.py')
WEB = ('index.html', 'app.js', 'worker.js', 'gr1-use-original.txt')


def build(bundle, output):
    spec = importlib.util.spec_from_file_location('ci_bundle', ROOT / 'tools/ci-bundle.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.read_download(bundle)
    output.mkdir(parents=True, exist_ok=False)
    for directory, names in [('web', WEB), ('tools', TOOLS)]:
        (output / directory).mkdir()
        for name in names:
            shutil.copyfile(ROOT / directory / name, output / directory / name)
    shutil.copytree(ROOT / 'web/images', output / 'web/images')
    shutil.copyfile(bundle, output / 'web/gr1-ui-download.tar.gz')
    (output / 'index.html').write_text('<!doctype html><meta http-equiv="refresh" content="0;url=web/"><a href="web/">GR-1 interface</a>\n')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bundle', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    build(args.bundle, args.output)
