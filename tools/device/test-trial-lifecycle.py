#!/usr/bin/env python3
"""Run the unchanged trial wrapper under Linux dash with a harmless child.

Run in the cached build container; no native engine or real service runs.
Service identity, process-name discovery and ALSA readiness are command
substitutes. Signals, child wait, tail and bounded termination are real.
"""
import importlib.util
import os
from pathlib import Path
import shutil
import signal
import subprocess
import time
import unittest


TOOLS = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('stock_checks', TOOLS / 'test-stock-service.py')
stock_checks = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stock_checks)


class TrialLifecycleChecks(unittest.TestCase):
    def setUp(self):
        if not Path('/proc/self/stat').exists() or os.getuid() != 0:
            self.skipTest('requires the isolated Linux build container as root')
        self.fixture = stock_checks.StockServiceChecks('test_original_service_is_accepted')
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.root = Path(self.fixture.scratch.name)
        self.package = self.root / 'package'
        self.package.mkdir()
        for name in ['temporary-trial.sh', 'stock-service.sh']:
            shutil.copyfile(TOOLS / name, self.package / name)
        self.data = Path('/media/internal/gr1-ui-repair-data') / f'controlled-{os.getpid()}'
        (self.data / 'performance01').mkdir(parents=True)
        (self.data / 'performance01/performance.grm').write_text('controlled fixture\n')
        self.addCleanup(shutil.rmtree, self.data)
        self.pidfile = self.root / 'child.pid'
        self.output = self.root / 'wrapper.log'
        self.env = dict(self.fixture.env, CHECK_CHILD_PIDFILE=str(self.pidfile),
                        CHECK_CHILD_SCRIPT=str(self.package / 'harmless-child.py'))
        self.child_source = '''import os, signal, sys, time
from pathlib import Path
if '--check-device' in sys.argv:
    raise SystemExit(0)
mode = os.environ.get('CHECK_CHILD_MODE', 'wait')
if mode == 'hung':
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
Path(os.environ['CHECK_CHILD_PIDFILE']).write_text(str(os.getpid()))
print('Harmless child ready', flush=True)
if mode in ('normal', 'failure'):
    time.sleep(0.15)
    raise SystemExit(0 if mode == 'normal' else 7)
while True:
    time.sleep(0.1)
'''
        (self.package / 'harmless-child.py').write_text(self.child_source)
        self.command(self.package / 'run-device.sh', '#!/bin/sh\nexec python3 "$(dirname "$0")/harmless-child.py" "$@"\n')
        self.command(self.root / 'sleep', '#!/bin/sh\nexec /bin/sleep "$@"\n')
        self.command(self.root / 'sha256sum', '''#!/bin/sh
if [ "$1" = -c ]; then exec /usr/bin/sha256sum "$@"; fi
echo "$CHECK_SERVICE_HASH  $1"
''')
        self.command(self.root / 'pgrep', '''#!/usr/bin/python3
import os, sys
from pathlib import Path
if sys.argv[1:] == ['-x', 'grmain.exe']:
    raise SystemExit(0)
pidfile = Path(os.environ['CHECK_CHILD_PIDFILE'])
if pidfile.exists():
    pid = pidfile.read_text()
    try:
        state = Path('/proc/' + pid + '/stat').read_text().split(') ', 1)[1][0]
        command = Path('/proc/' + pid + '/cmdline').read_bytes()
        if state != 'Z' and os.environ['CHECK_CHILD_SCRIPT'].encode() in command:
            raise SystemExit(0)
    except FileNotFoundError:
        pass
raise SystemExit(1)
''')
        self.command(self.root / 'systemctl', '''#!/bin/sh
if [ "$1" = show ]; then echo 'DropInPaths='; exit 0; fi
if [ "$1" = stop ]; then echo stop >> "$CHECK_LOG"; exit 0; fi
if [ "$1" = --no-block ] && [ "$2" = start ]; then
    if pgrep '^gr1-ui' >/dev/null; then
        echo competing-host >> "$CHECK_LOG"; exit 42
    fi
    echo start >> "$CHECK_LOG"
    echo 0 > "$CHECK_CLOCK"
    exit 0
fi
exit 2
''')
        # The package manifest itself is checked by the actual sha256sum tool.
        result = subprocess.run(['/usr/bin/sha256sum', 'temporary-trial.sh',
                                 'stock-service.sh', 'run-device.sh', 'harmless-child.py'],
                                cwd=self.package, text=True, capture_output=True, check=True)
        (self.package / 'SHA256SUMS').write_text(result.stdout)
        self.process = None
        self.owned = []
        self.addCleanup(self.stop_owned_processes)

    @staticmethod
    def command(path, source):
        path.write_text(source)
        path.chmod(0o755)

    @staticmethod
    def running(pid):
        try:
            return Path(f'/proc/{pid}/stat').read_text().split(') ', 1)[1][0] != 'Z'
        except FileNotFoundError:
            return False

    def stop_owned_processes(self):
        for pid in self.owned:
            if self.running(pid):
                os.kill(pid, signal.SIGKILL)
        if self.process is not None and self.process.poll() is None:
            self.process.kill()
            self.process.wait(timeout=2)

    def run_case(self, mode='wait', sig=None):
        with self.output.open('w') as output:
            self.process = subprocess.Popen(['/bin/sh', str(self.package / 'temporary-trial.sh'),
                                             str(self.data)], stdin=subprocess.DEVNULL,
                                            stdout=output, stderr=subprocess.STDOUT,
                                            env=dict(self.env, CHECK_CHILD_MODE=mode),
                                            start_new_session=True)
            deadline = time.monotonic() + 2
            while not self.pidfile.exists() and self.process.poll() is None and time.monotonic() < deadline:
                time.sleep(0.005)
            self.assertTrue(self.pidfile.exists(), self.output.read_text())
            child = int(self.pidfile.read_text())
            self.owned.append(child)
            children = Path(f'/proc/{self.process.pid}/task/{self.process.pid}/children').read_text().split()
            tails = [int(pid) for pid in children if Path(f'/proc/{pid}/comm').read_text().strip() == 'tail']
            self.owned.extend(tails)
            self.assertEqual(len(tails), 1)
            started = time.monotonic()
            if sig is not None:
                os.kill(self.process.pid, sig)
            status = self.process.wait(timeout=8)
            elapsed = time.monotonic() - started
        # Observe descendants separately; wrapper status alone cannot prove cleanup.
        for pid in [child, *tails]:
            deadline = time.monotonic() + 1
            while self.running(pid) and time.monotonic() < deadline:
                time.sleep(0.005)
            self.assertFalse(self.running(pid), f'orphan PID {pid}: {self.output.read_text()}')
        self.assertEqual(self.fixture.log.read_text(), 'stop\nstart\n', self.output.read_text())
        self.assertIn('Trial ended.', self.output.read_text())
        print(f'dash lifecycle mode={mode} signal={sig}: status={status}, elapsed={elapsed:.3f}s, child/tail stopped, exactly one stock start', flush=True)
        return status, elapsed

    def test_wrapper_term_cleans_up_child_and_tail(self):
        self.assertEqual(self.run_case(sig=signal.SIGTERM)[0], 130)

    def test_wrapper_hup_cleans_up_child_and_tail(self):
        self.assertEqual(self.run_case(sig=signal.SIGHUP)[0], 130)

    def test_wrapper_int_cleans_up_child_and_tail(self):
        self.assertEqual(self.run_case(sig=signal.SIGINT)[0], 130)

    def test_child_normal_exit_restores_stock(self):
        self.assertEqual(self.run_case(mode='normal')[0], 0)

    def test_child_failure_restores_stock_and_preserves_failure(self):
        self.assertEqual(self.run_case(mode='failure')[0], 7)

    def test_hung_child_is_killed_before_stock_restart(self):
        status, elapsed = self.run_case(mode='hung', sig=signal.SIGTERM)
        self.assertEqual(status, 130)
        self.assertGreaterEqual(elapsed, 4.5)
        self.assertLess(elapsed, 7)


if __name__ == '__main__':
    unittest.main(verbosity=2)
