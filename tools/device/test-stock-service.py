#!/usr/bin/env python3
"""Check stock service guards with command substitutes, never device/engine code."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


HELPER = Path(__file__).with_name('stock-service.sh').resolve()
SERVICE_HASH = '96350a1360b3df90d8c19318b1798bda954d675405ec347a673774f9712ee12f'


class StockServiceChecks(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix='gr1-stock-check-')
        self.addCleanup(self.scratch.cleanup)
        root = Path(self.scratch.name)
        commands = {
            'systemctl': '''#!/bin/sh
if [ "$1" = show ]; then
  if [ "${CHECK_QUERY_STATUS:-0}" != 0 ]; then
    echo 'query failed' >&2
    exit "$CHECK_QUERY_STATUS"
  fi
  echo "DropInPaths=${CHECK_OVERRIDES:-}"
else
  echo start >> "$CHECK_LOG"
  echo 0 > "$CHECK_CLOCK"
  exit "${CHECK_START_STATUS:-0}"
fi
''',
            'sha256sum': '#!/bin/sh\necho "$CHECK_SERVICE_HASH  $1"\nexit "${CHECK_HASH_STATUS:-0}"\n',
            'pgrep': '''#!/bin/sh
case "$*" in
  '-x grmain.exe')
    [ "${CHECK_STOCK_PRESENT:-1}" = 1 ] &&
      [ "$(cat "$CHECK_CLOCK" 2>/dev/null || echo 0)" -ge "${CHECK_STOCK_AFTER:-0}" ] ;;
  *) [ "${CHECK_OTHER_HOST:-0}" = 1 ] ;;
esac
''',
            'head': '''#!/bin/sh
clock=$(cat "$CHECK_CLOCK" 2>/dev/null || echo 0)
if [ "$clock" -lt "${CHECK_AUDIO_AFTER:-0}" ]; then
  echo 'state: CLOSED'
else
  echo "state: ${CHECK_AUDIO:-RUNNING}"
fi
''',
            'sleep': '''#!/bin/sh
clock=$(cat "$CHECK_CLOCK" 2>/dev/null || echo 0)
echo "$((clock + $1))" > "$CHECK_CLOCK"
''',
        }
        for name, source in commands.items():
            path = root / name
            path.write_text(source)
            path.chmod(0o755)
        self.log = root / 'calls'
        self.env = dict(os.environ, PATH=f'{root}:/usr/bin:/bin',
                        CHECK_LOG=str(self.log), CHECK_CLOCK=str(root / 'clock'),
                        CHECK_SERVICE_HASH=SERVICE_HASH)

    def run_helper(self, function, **changes):
        env = dict(self.env, **changes)
        return subprocess.run(['sh', '-c', '. "$1"; "$2"', 'check',
                               str(HELPER), function], env=env,
                              text=True, capture_output=True, timeout=5)

    def test_original_service_is_accepted(self):
        self.assertEqual(self.run_helper('gr1_check_stock_startup').returncode, 0)

    def test_override_is_rejected_before_start(self):
        result = self.run_helper('gr1_restore_stock', CHECK_OVERRIDES='/etc/rust.conf')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('override', result.stdout)
        self.assertFalse(self.log.exists())

    def test_changed_service_is_rejected(self):
        result = self.run_helper('gr1_check_stock_startup', CHECK_SERVICE_HASH='changed')
        self.assertNotEqual(result.returncode, 0)

    def test_failed_override_query_withholds_service_start(self):
        result = self.run_helper('gr1_restore_stock', CHECK_QUERY_STATUS='1')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Cannot query', result.stdout)
        self.assertFalse(self.log.exists())

    def test_failed_hash_query_withholds_service_start(self):
        result = self.run_helper('gr1_restore_stock', CHECK_HASH_STATUS='1')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Cannot read', result.stdout)
        self.assertFalse(self.log.exists())

    def test_other_host_withholds_stock_start(self):
        result = self.run_helper('gr1_restore_stock', CHECK_OTHER_HOST='1')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.log.exists())

    def test_service_start_failure_fails_recovery(self):
        self.assertNotEqual(self.run_helper('gr1_restore_stock',
                                           CHECK_START_STATUS='1').returncode, 0)

    def test_missing_stock_fails_recovery(self):
        result = self.run_helper('gr1_restore_stock', CHECK_STOCK_PRESENT='0')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('recovery failed', result.stdout)

    def test_closed_audio_fails_recovery(self):
        result = self.run_helper('gr1_restore_stock', CHECK_AUDIO='CLOSED')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('recovery failed', result.stdout)

    def test_process_and_audio_recovery_passes(self):
        result = self.run_helper('gr1_restore_stock')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.log.read_text(), 'start\n')
        self.assertIn('physical screen and sound', result.stdout)

    def test_audio_that_returns_after_five_seconds_is_observed(self):
        result = self.run_helper('gr1_restore_stock', CHECK_AUDIO_AFTER='8')
        print(f'Delayed audio: status={result.returncode}, clock={Path(self.env["CHECK_CLOCK"]).read_text().strip()}, output={result.stdout.strip()}')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.log.read_text(), 'start\n')

    def test_process_and_audio_can_both_arrive_later(self):
        result = self.run_helper('gr1_restore_stock', CHECK_STOCK_AFTER='6',
                                 CHECK_AUDIO_AFTER='8')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(Path(self.env['CHECK_CLOCK']).read_text().strip(), '8')

    def test_audio_beyond_the_readiness_deadline_is_a_failure(self):
        result = self.run_helper('gr1_restore_stock', CHECK_AUDIO_AFTER='31')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('within 30 seconds', result.stdout)
        self.assertEqual(Path(self.env['CHECK_CLOCK']).read_text().strip(), '30')

    def test_missing_stock_stops_at_the_readiness_deadline(self):
        result = self.run_helper('gr1_restore_stock', CHECK_STOCK_PRESENT='0')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Stock process did not return within 30 seconds', result.stdout)
        self.assertEqual(Path(self.env['CHECK_CLOCK']).read_text().strip(), '30')


if __name__ == '__main__':
    unittest.main(verbosity=2)
