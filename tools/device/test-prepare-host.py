#!/usr/bin/env python3
"""Exercise startup setup with isolated paths and commands; never run an engine."""
import hashlib
import json
import os
from pathlib import Path
import shlex
import subprocess
import tarfile
import tempfile
import unittest


SOURCE = Path(__file__).resolve().parent
REPO = SOURCE.parent.parent
MODEL = 'Raspberry Pi 3 Model B Plus Rev 1.3'


class PrepareHostChecks(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix='gr1-startup-check-')
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        self.commands = self.root / 'commands'
        self.commands.mkdir()
        for name, text in {
            'id': '#!/bin/sh\necho "${CHECK_UID:-0}"\n',
            'uname': '#!/bin/sh\ncase "$1" in -s) echo "${CHECK_OS:-Linux}";; -m) echo "${CHECK_ARCH:-armv7l}";; esac\n',
            'setserial': '#!/bin/sh\nprintf "%s\\n" "$@" > "$CHECK_SERIAL_LOG"\nexit "${CHECK_SERIAL_STATUS:-0}"\n',
        }.items():
            path = self.commands / name
            path.write_text(text)
            path.chmod(0o755)
        self.model = self.root / 'model'
        self.model.write_text(MODEL + '\0')
        self.prealloc = self.root / 'prealloc'
        self.prealloc.write_text('old preallocation\n')
        self.cpus = self.root / 'cpus'
        for cpu in range(4):
            path = self.cpus / f'cpu{cpu}/cpufreq/scaling_governor'
            path.parent.mkdir(parents=True)
            path.write_text('powersave\n')
        self.serial_log = self.root / 'serial-log'
        # Only this test copy redirects fixed production paths. No runtime
        # environment variable can redirect the shipped helper's writes.
        source = (SOURCE / 'prepare-host.sh').read_text()
        redirects = {
            '/proc/device-tree/model': str(self.model),
            '/proc/asound/card0/pcm0p/sub0/prealloc': str(self.prealloc),
            '/sys/devices/system/cpu': str(self.cpus),
            '/bin/setserial': str(self.commands / 'setserial'),
        }
        for original, replacement in redirects.items():
            self.assertIn(original, source)
            source = source.replace(original, replacement)
        self.helper = self.root / 'prepare-host.sh'
        self.helper.write_text(source)
        self.env = dict(os.environ, PATH=f'{self.commands}:/usr/bin:/bin',
                        CHECK_SERIAL_LOG=str(self.serial_log))

    def run_helper(self, **changes):
        return subprocess.run(['/bin/sh', str(self.helper)],
                              env=dict(self.env, **changes), text=True,
                              capture_output=True, timeout=5)

    def test_stock_setup_values_and_uart_arguments(self):
        result = self.run_helper()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.prealloc.read_text(), '4096\n')
        for path in self.cpus.glob('cpu*/cpufreq/scaling_governor'):
            self.assertEqual(path.read_text(), 'performance\n')
        self.assertEqual(self.serial_log.read_text(), '/dev/ttyAMA0\nlow_latency\n')

    def test_absent_optional_preallocation_is_skipped(self):
        self.prealloc.unlink()
        self.assertEqual(self.run_helper().returncode, 0)
        self.assertFalse(self.prealloc.exists())
        self.assertTrue(self.serial_log.exists())

    def test_wrong_host_or_authority_stops_before_writes(self):
        for changes in ({'CHECK_UID': '1000'}, {'CHECK_OS': 'Darwin'},
                        {'CHECK_ARCH': 'x86_64'}):
            with self.subTest(changes=changes):
                self.assertNotEqual(self.run_helper(**changes).returncode, 0)
                self.assertEqual(self.prealloc.read_text(), 'old preallocation\n')
                self.assertFalse(self.serial_log.exists())
        self.model.write_text('Generic Pi\n')
        self.assertNotEqual(self.run_helper().returncode, 0)
        self.assertEqual(self.prealloc.read_text(), 'old preallocation\n')
        self.assertFalse(self.serial_log.exists())

    def test_setup_failure_withholds_startup_success(self):
        self.assertNotEqual(self.run_helper(CHECK_SERIAL_STATUS='1').returncode, 0)

    def test_foreground_process_gets_original_locale_and_arguments(self):
        package = self.root / 'process'
        package.mkdir()
        (package / 'run-device.sh').write_bytes((SOURCE / 'run-device.sh').read_bytes())
        # This executable reports environment/arguments; it is not Rust or native.
        executable = package / 'gr1-ui'
        executable.write_text('#!/bin/sh\nprintf "%s\\n" "$LANG" "$LC_ALL" "$@"\n')
        executable.chmod(0o755)
        result = subprocess.run(['/bin/sh', str(package / 'run-device.sh'),
                                 '--data-root', '/fixture/data'],
                                text=True, capture_output=True, timeout=5)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.splitlines(), [
            'en_US.UTF-8', 'en_US.UTF-8', '--device', '--engine',
            str(package / 'lib/libgr.so.1.0.1'), '--data-root', '/fixture/data'])

    def test_versioned_override_generator_and_template_prestart(self):
        # Execute the actual override-emission fragment, bypassing installation.
        script = (SOURCE / 'install-boot.sh').read_text()
        fragment = script.split('cat > "$override" <<EOF\n', 1)[1].split('\nEOF\n', 1)[0]
        override = self.root / 'override'
        package = '/media/internal/gr1-ui-installed/' + 'a' * 40
        command = (f'package_dir={shlex.quote(package)}\n'
                   f'cat > {shlex.quote(str(override))} <<EOF\n{fragment}\nEOF\n')
        subprocess.run(['/bin/sh', '-c', command], check=True, timeout=5)
        lines = override.read_text().splitlines()
        self.assertIn(f'ExecStartPre={package}/prepare-host.sh', lines)
        self.assertIn(f'ExecStart={package}/run-device.sh --data-root /media/internal', lines)
        self.assertLess(lines.index(f'ExecStartPre={package}/prepare-host.sh'),
                        lines.index(f'ExecStart={package}/run-device.sh --data-root /media/internal'))
        self.assertIn('ExecStartPre=/media/internal/gr1-ui/prepare-host.sh',
                      (SOURCE / '90-rust-ui.conf').read_text().splitlines())

    def test_packager_includes_executable_hashed_helper(self):
        build = self.root / 'build'
        package = build / 'package'
        # Required binary and license inputs are inert text fixtures; none run.
        for relative in ('package/gr1-ui', 'package/lib/libSDL2-2.0.so.0',
                         'package/lib/libgr.so.1.0.1',
                         'sysroot/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf',
                         'sysroot/usr/share/doc/fonts-dejavu-core/copyright'):
            path = build / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('fixture only\n')
        sdl = self.root / 'sdl'
        sdl.mkdir()
        (sdl / 'LICENSE.txt').write_text('fixture license\n')
        result = subprocess.run(['python3', str(SOURCE / 'package.py'),
                                 '--root', str(build), '--sdl-source', str(sdl)],
                                cwd=REPO, text=True, capture_output=True, timeout=15)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        helper = package / 'prepare-host.sh'
        self.assertEqual(helper.read_bytes(), (SOURCE / 'prepare-host.sh').read_bytes())
        self.assertTrue(helper.stat().st_mode & 0o111)
        digest = hashlib.sha256(helper.read_bytes()).hexdigest()
        self.assertEqual(json.loads((package / 'manifest.json').read_text())
                         ['files_sha256']['prepare-host.sh'], digest)
        self.assertIn(f'{digest}  prepare-host.sh\n', (package / 'SHA256SUMS').read_text())
        with tarfile.open(build / 'gr1-ui-3.4.4-armhf.tar.gz') as archive:
            self.assertEqual(archive.extractfile('gr1-ui/prepare-host.sh').read(),
                             helper.read_bytes())


if __name__ == '__main__':
    unittest.main(verbosity=2)
