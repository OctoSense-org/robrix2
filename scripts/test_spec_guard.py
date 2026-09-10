"""Subprocess regressions for the real spec-guard entry point; no live services."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent
FAKE_CLI = '''#!/usr/bin/env python3
import json, pathlib, sys
a = sys.argv[1:]
root = pathlib.Path.cwd()
with (root / 'calls.jsonl').open('a') as log: log.write(json.dumps(a) + '\\n')
if a[0] in ['lint', 'check-structure', 'trace']: sys.exit(0)
doc = json.loads(pathlib.Path(a[1]).read_text())
if a[0] == 'parse':
    print(json.dumps({'sections': [{'kind': 'boundaries', 'items':
        [{'text': p, 'category': 'allow'} for p in doc['allow']] +
        [{'text': p, 'category': 'deny'} for p in doc.get('deny', [])]}]})); sys.exit(0)
if doc.get('malformed'): print('not JSON'); sys.exit(0)
changes = [a[i+1] for i,x in enumerate(a) if x == '--change']
bad = [p for p in changes if p not in doc['allow'] or p in doc.get('deny', [])]
n = doc.get('failed', 0) + bool(bad)
results = [{'scenario_name': 'failure ' + str(i), 'verdict': 'fail',
    'step_results': [{'step_text': p, 'verdict': 'fail', 'reason': 'not covered by any allowed boundary'} for p in bad]}
    for i in range(n)]
summary = {'total': 1+n+doc.get('skipped', 0)+doc.get('uncertain', 0), 'passed': 1,
    'failed': n, 'skipped': doc.get('skipped', 0), 'uncertain': doc.get('uncertain', 0)}
report = {'summary': summary, 'results': results}
print(json.dumps({'verification': report} if a[0] == 'lifecycle' else report, indent=2))
sys.exit(1 if n or summary['skipped'] or summary['uncertain'] else 0)
'''


class GuardTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='robrix-spec-guard-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for directory in ['scripts', 'specs', 'src', 'bin']:
            (self.root / directory).mkdir()
        for name in ['spec-guard.sh', 'spec_guard.py']:
            source = ROOT / 'scripts' / name
            if source.exists(): shutil.copy2(source, self.root / 'scripts' / name)
        cli = self.root / 'bin/agent-spec'
        cli.write_text(FAKE_CLI)
        cli.chmod(0o755)
        self.env = dict(os.environ, PATH=str(self.root / 'bin') + os.pathsep + os.environ['PATH'])
        subprocess.run(['git', 'init', '-q', str(self.root)], check=True)

    def contract(self, name, paths, changed=True, **options):
        spec = 'specs/' + name + '.spec.md'
        doc = {'allow': [spec, *paths], **options}
        (self.root / spec).write_text(json.dumps(doc))
        for path in paths:
            target = self.root / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text('// fixture\n')
        if changed:
            subprocess.run(['git', 'add', spec, *paths], cwd=self.root, check=True)
        return spec

    def run_guard(self):
        # macOS's system Bash lacks mapfile; use the same Bash 5 entry point as local CI.
        bash = '/opt/homebrew/bin/bash' if Path('/opt/homebrew/bin/bash').exists() else 'bash'
        result = subprocess.run([bash, 'scripts/spec-guard.sh'], cwd=self.root,
            env=self.env, text=True, capture_output=True, timeout=30)
        calls = [json.loads(line) for line in (self.root / 'calls.jsonl').read_text().splitlines()]
        return result, calls

    def owned(self, calls, spec):
        calls = [c for c in calls if c[:2] == ['lifecycle', spec]]
        self.assertEqual(len(calls), 1)
        return {calls[0][i+1] for i, a in enumerate(calls[0]) if a == '--change'}

    def test_disjoint_and_unchanged_contracts(self):
        a = self.contract('task-a', ['src/a.rs'])
        b = self.contract('task-b', ['src/b.rs'])
        c = self.contract('task-old', [], changed=False)
        result, calls = self.run_guard()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.owned(calls, a), {a, 'src/a.rs'})
        self.assertEqual(self.owned(calls, b), {b, 'src/b.rs'})
        self.assertTrue(any(ca[:2] == ['verify', c] for ca in calls))

    def test_unowned_file_rejected(self):
        self.contract('task-a', ['src/a.rs'])
        self.contract('task-b', ['src/b.rs'])
        (self.root / 'src/unowned.rs').write_text('// outside all contracts\n')
        subprocess.run(['git', 'add', 'src/unowned.rs'], cwd=self.root, check=True)
        result, _ = self.run_guard()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('src/unowned.rs', result.stdout + result.stderr)

    def test_forbidden_path_cannot_be_owned_by_another_contract(self):
        self.contract('task-a', ['src/a.rs'], deny=['src/b.rs'])
        self.contract('task-b', ['src/b.rs'])
        result, _ = self.run_guard()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('src/b.rs', result.stdout + result.stderr)

    def test_failed_contract_does_not_stop_later_checks(self):
        self.contract('task-a', ['src/shared.rs'], failed=300)
        b = self.contract('task-b', ['src/shared.rs'])
        result, calls = self.run_guard()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('spec-guard: FAILED', result.stdout)
        self.assertEqual(self.owned(calls, b), {b, 'src/shared.rs'})
        self.assertNotIn('Broken pipe', result.stderr)

    def test_malformed_report_fails_and_continues(self):
        self.contract('task-a', ['src/shared.rs'], malformed=True)
        b = self.contract('task-b', ['src/shared.rs'])
        result, calls = self.run_guard()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.owned(calls, b), {b, 'src/shared.rs'})

    def test_single_contract_receives_full_change_set(self):
        a = self.contract('task-a', ['src/a.rs'])
        (self.root / 'src/outside.rs').write_text('// outside\n')
        subprocess.run(['git', 'add', 'src/outside.rs'], cwd=self.root, check=True)
        result, calls = self.run_guard()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.owned(calls, a), {a, 'src/a.rs', 'src/outside.rs'})

    def test_manual_skip_is_reported_without_becoming_failure(self):
        self.contract('task-a', ['src/a.rs'], skipped=1)
        result, _ = self.run_guard()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('skipped=1', result.stdout)

    def test_uncertain_regression_remains_visibly_unverified(self):
        self.contract('task-a', ['src/a.rs'])
        old = self.contract('task-old', [], changed=False, uncertain=5)
        result, _ = self.run_guard()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('UNVERIFIED (regression): ' + old, result.stdout)
        self.assertIn('uncertain=5', result.stdout)
        self.assertNotIn('ok (regression): ' + old, result.stdout)

    def test_uncertain_active_contract_fails(self):
        active = self.contract('task-a', ['src/a.rs'], uncertain=1)
        result, _ = self.run_guard()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('FAIL (contract): ' + active, result.stdout)
        self.assertIn('uncertain=1', result.stdout)

    def generated(self, count, unowned):
        contracts = [self.contract('task-' + str(i), ['src/shared.rs', 'src/' + str(i) + '.rs']) for i in range(count)]
        if unowned:
            (self.root / 'src/unowned.rs').write_text('// unowned\n')
            subprocess.run(['git', 'add', 'src/unowned.rs'], cwd=self.root, check=True)
        result, calls = self.run_guard()
        self.assertEqual(result.returncode == 0, not unowned, result.stdout + result.stderr)
        if not unowned:
            for i, spec in enumerate(contracts):
                self.assertEqual(self.owned(calls, spec), {spec, 'src/shared.rs', 'src/' + str(i) + '.rs'})


if __name__ == '__main__':
    if len(sys.argv) == 4 and sys.argv[1] == '--generated':
        case = GuardTests()
        try:
            case.setUp()
            case.generated(int(sys.argv[2]), sys.argv[3] == '1')
        finally:
            case.doCleanups()
    else:
        unittest.main()
