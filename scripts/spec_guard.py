"""Task composition for spec-guard.sh. Native agent-spec still verifies each scope."""
import argparse
from functools import lru_cache
import json
from pathlib import Path
import re
import subprocess
import sys


def path_pattern(text):
    # Match the path forms recognized by agent-spec; prose is not an allow-list.
    text = text.strip()
    if not (any(c in text for c in '/\\*') or text.endswith(('.rs', '.ts', '.js', '.py', '.md', '.spec'))):
        return None
    text = text.strip('`').replace('\\', '/')
    while text.startswith('./'):
        text = text[2:]
    return text.strip('/')


def matches(pattern, path):
    pattern = tuple(p for p in pattern.split('/') if p)
    path = tuple(p for p in path.split('/') if p)

    @lru_cache(None)
    def visit(i, j):
        if i == len(pattern):
            return j == len(path)
        if pattern[i] == '**':
            return visit(i + 1, j) or (j < len(path) and visit(i, j + 1))
        return (j < len(path)
            and re.fullmatch(re.escape(pattern[i]).replace(r'\*', '.*'), path[j]) is not None
            and visit(i + 1, j + 1))

    return visit(0, 0)


def partition_changes(changes, documents):
    """All owners receive shared files; unowned paths and explicit denies fail."""
    scopes = {spec: [] for spec in documents}
    rules = {}
    for spec, document in documents.items():
        allow, deny = [], []
        for section in document['sections']:
            if section['kind'] != 'boundaries':
                continue
            for item in section['items']:
                pattern = path_pattern(item['text'])
                if pattern and item['category'] in ('allow', 'deny', 'general'):
                    (allow if item['category'] == 'allow' else deny).append(pattern)
        rules[spec] = (allow, deny)
    errors = []
    for change in changes:
        owned = False
        for spec, (allow, deny) in rules.items():
            if any(matches(pattern, change) for pattern in deny):
                errors.append(f'{change}: forbidden by {spec}')
            if any(matches(pattern, change) for pattern in allow):
                scopes[spec].append(change)
                owned = True
        if not owned:
            errors.append(f'{change}: not covered by any active task boundary')
    if errors:
        raise ValueError('\n'.join(errors))
    return scopes


def cli_json(arguments):
    result = subprocess.run(['agent-spec', *arguments], text=True, capture_output=True)
    try:
        document = json.loads(result.stdout)
    except (ValueError, TypeError) as error:
        raise ValueError(f'invalid agent-spec JSON (exit {result.returncode}): '
            f'{(result.stderr or result.stdout)[:1200]}') from error
    if not isinstance(document, dict):
        raise ValueError('agent-spec report must be an object')
    return result.returncode, document


def check_report(code, document, regression=False):
    report = document.get('verification', document)
    summary = report['summary']
    counts = {key: summary[key] for key in ('total', 'passed', 'failed', 'skipped', 'uncertain')}
    if any(type(value) is not int or value < 0 for value in counts.values()):
        raise ValueError('invalid verification counts')
    # Preserve the existing nonblocking uncertainty policy for unchanged tasks.
    # Those results remain UNVERIFIED; active contracts cannot use this policy.
    permitted_uncertain = regression and counts['uncertain'] > 0
    unverified = counts['skipped'] > 0 or counts['uncertain'] > 0
    ok = (code in (0, 1) and counts['total'] > 0 and counts['failed'] == 0
        and (counts['uncertain'] == 0 or permitted_uncertain)
        and (code == 0 or counts['skipped'] > 0 or permitted_uncertain))
    detail = ', '.join(f'{key}={counts[key]}' for key in ('passed', 'failed', 'skipped', 'uncertain'))
    if not ok or counts['uncertain'] > 0:
        # Slice parsed diagnostics, never a live stdout pipe. Later tasks still run.
        failures = [r for r in report.get('results', []) if r.get('verdict') in ('fail', 'uncertain')]
        for result in failures[:12]:
            print('  ' + result.get('scenario_name', '<unnamed>'))
            for step in result.get('step_results', [])[:3]:
                if step.get('verdict') in ('fail', 'uncertain'):
                    print(f"    {step.get('step_text', '')}: {step.get('reason', '')}"[:1200])
    return ok, detail, unverified


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--change', action='append', default=[])
    args = parser.parse_args()
    changes = list(dict.fromkeys(args.change))
    specs = sorted(str(p) for p in Path('specs').glob('*.spec.md') if p.name != 'project.spec.md')
    active = [spec for spec in specs if spec in changes]
    scopes = {spec: changes for spec in active}
    if len(active) > 1:
        try:
            documents = {}
            for spec in active:
                code, document = cli_json(['parse', spec, '--format', 'json'])
                if code != 0:
                    raise ValueError(f'could not parse {spec}: exit {code}')
                documents[spec] = document
            scopes = partition_changes(changes, documents)
        except (ValueError, KeyError, TypeError) as error:
            print(f'FAIL (composition): {error}', file=sys.stderr)
            return 1
    failed = False
    for spec in specs:
        changed = spec in scopes
        mode = 'contract' if changed else 'regression'
        arguments = ['lifecycle' if changed else 'verify', spec, '--code', '.', '--format', 'json']
        if changed:
            arguments += ['--run-log-dir', '.agent-spec/runs']
            for change in scopes[spec]:
                arguments += ['--change', change]
        try:
            code, document = cli_json(arguments)
            ok, detail, unverified = check_report(code, document, regression=not changed)
        except (ValueError, KeyError, TypeError) as error:
            ok, detail, unverified = False, str(error), False
        status = ('UNVERIFIED' if unverified else 'ok') if ok else 'FAIL'
        print(f'{status} ({mode}): {spec} — {detail}', flush=True)
        if ok and unverified:
            print('  Nonblocking under existing CI policy; skipped/uncertain scenarios are not passes.', flush=True)
        failed |= not ok
    return int(failed)


if __name__ == '__main__':
    sys.exit(main())
