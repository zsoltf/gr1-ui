#!/usr/bin/env python3
"""Exercise archive layout and boot routing with inert hosts, never an engine."""
import importlib.util
import io
from pathlib import Path
import os
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('usb', Path(__file__).with_name('usb-update.py'))
usb = importlib.util.module_from_spec(spec)
spec.loader.exec_module(usb)


class UsbChecks(unittest.TestCase):
    def test_update_retains_stock_and_uses_updater_root_layout(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            stock = {'grui/run.sh': b'original launcher\n', 'grui/graphics': b'inert vendor data',
                     'root/rungr.new': b'original startup\n', 'etc/example': b'original config\n'}
            with tarfile.open(root / 'stock.tgz', 'w:gz') as t:
                for name, data in stock.items():
                    member = tarfile.TarInfo(name)
                    member.size, member.mode = len(data), 0o755
                    t.addfile(member, io.BytesIO(data))
            payload = {name: (b'inert authored payload', 0o755) for name in usb.RUNTIME}
            payload['SOURCE_REVISION'] = (b'a' * 40 + b'\n', 0o644)
            with patch.object(usb.core, 'prepare_files', return_value=payload):
                result = usb.prepare(root / 'unused', root / 'stock.tgz', root / 'update.tgz')
            self.assertEqual(result['filename'], 'gr1_firmware_3.4.4_ui_aaaaaaa.tgz')
            with tarfile.open(root / 'update.tgz') as t:
                files = {m.name: t.extractfile(m).read() for m in t}
            for name, data in stock.items():
                self.assertEqual(files['grui/run-stock.sh' if name == 'grui/run.sh' else name], data)
            self.assertEqual(files['grui/run.sh'], usb.LAUNCHER)
            self.assertIn('grui/gr1-ui/gr1-ui', files)
            self.assertFalse(any(name.startswith('gr1-ui/') for name in files))

    def test_boot_selects_one_host_and_preserves_usb_recovery(self):
        # Substitute only machine paths and external hardware/checksum commands.
        # Both hosts print an identity; no vendor or ARM code is executed.
        cases = [('normal', None, True, 'new'), ('recovery', usb.RECOVERY_NAME, True, 'stock'),
                 ('update', 'gr1_firmware_3.4.4_test.tgz', True, 'stock'),
                 ('damaged', None, False, 'stock')]
        for name, marker, healthy, expected in cases:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                (root / 'gr1-ui').mkdir()
                (root / 'bin').mkdir()
                drive = root / 'usb/.usb0'
                drive.mkdir(parents=True)
                if marker:
                    (drive / marker).touch()
                (root / 'model').write_text('Raspberry Pi 3 Model B Plus Rev 1.3')
                (root / 'run-stock.sh').write_text('#!/bin/sh\necho stock\n')
                new = root / 'gr1-ui/run-device.sh'
                new.write_text('#!/bin/sh\necho new\n')
                new.chmod(0o755)
                for command, body in {'uname': 'echo armv7l', 'sleep': ':',
                                      'sha256sum': 'exit ' + ('0' if healthy else '1')}.items():
                    file = root / 'bin' / command
                    file.write_text('#!/bin/sh\n' + body + '\n')
                    file.chmod(0o755)
                launcher = usb.LAUNCHER.decode().replace('/home/sysop/GR-1/grui', str(root))
                launcher = launcher.replace('/proc/device-tree/model', str(root / 'model'))
                launcher = launcher.replace('/media/usb', str(root / 'usb'))
                (root / 'run.sh').write_text(launcher)
                env = dict(os.environ, PATH=str(root / 'bin') + ':' + os.environ['PATH'])
                result = subprocess.run(['/bin/sh', str(root / 'run.sh')], env=env,
                                        capture_output=True, text=True, timeout=5)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout.strip(), expected)


if __name__ == '__main__':
    unittest.main(verbosity=2)
