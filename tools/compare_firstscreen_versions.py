"""Counterbalanced same-session first-screen observations; never drop failed runs."""
import argparse
import hashlib
import json
import math
import pathlib
import statistics
import subprocess
import sys
import uuid

ROOT = pathlib.Path(__file__).resolve().parent.parent
p = argparse.ArgumentParser()
p.add_argument('--reference', type=pathlib.Path, required=True)
p.add_argument('--candidate', type=pathlib.Path, required=True)
p.add_argument('--output', type=pathlib.Path, required=True)
p.add_argument('--pairs', type=int, default=20)
p.add_argument('--pilot', action='store_true')
a = p.parse_args()
assert (a.pilot and a.pairs == 1) or (not a.pilot and a.pairs == 20)
output = a.output.resolve()
assert output.is_relative_to(ROOT / 'artifacts') and not output.exists()
deliveries = {'reference': a.reference.resolve(strict=True), 'candidate': a.candidate.resolve(strict=True)}
assert all(d.is_relative_to(ROOT / 'dist') for d in deliveries.values())
base = ROOT / 'target' / ('firstscreen-controlled-' + uuid.uuid4().hex)
base.mkdir()
result = {'finished': False, 'full_acceptance_pass': False, 'pairs_requested': a.pairs,
          'observation_seconds': 5, 'rows': [], 'child_reports': [], 'task': str(base),
          'physical_presentation_verified': False,
          'scope': 'Same-session counterbalanced version order, generated 5000-item library, fresh and reused app cache each pair, 5-second observation. All visible-completion conditions must be met, otherwise fail with original evidence retained. Main-entry/first-UI readiness proxies, not physical presentation, selection timing, cold boot or seek P95.'}


def read(p):
    return json.loads(p.read_text(encoding='utf-8-sig'))


def sha(p):
    with p.open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


result['exe_sha256'] = {name: read(d / 'delivery.json')['exe_sha256'] for name, d in deliveries.items()}
try:
    for pair in range(a.pairs):
        order = ('reference', 'candidate') if pair % 2 == 0 else ('candidate', 'reference')
        for version in order:
            report = ROOT / 'artifacts' / f'{output.stem}-{pair:02d}-{version}.json'
            log = base / f'{pair:02d}-{version}.log'
            args = [sys.executable, '-X', 'utf8', str(ROOT / 'tools/measure_firstscreen_pairs.py'),
                    '--delivery', str(deliveries[version]), '--pairs', '1', '--pilot', '--seconds', '5', '--output', str(report)]
            print(f'Pair {pair + 1}/{a.pairs}: {version}', flush=True)
            with log.open('wb') as out:
                proc = subprocess.Popen(args, cwd=ROOT, stdout=out, stderr=subprocess.STDOUT)
                result['active_child_pid'] = proc.pid
                output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf8')
                code = proc.wait(timeout=100)
            result['active_child_pid'] = None
            assert code == 0, f'Child failed; inspect {log} and {report}; no run discarded'
            d = read(report)
            assert d['finished'] and d['exe_sha256'] == result['exe_sha256'][version] and len(d['rows']) == 2
            result['child_reports'].append({'pair': pair + 1, 'version': version, 'path': str(report), 'sha256': sha(report)})
            for row in d['rows']:
                row.update(comparison_pair=pair + 1, version=version)
                assert row['exit_code'] == 0 and row['final']['metadata_ready'] < 5000
                result['rows'].append(row)
            output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf8')
    assert len({tuple(r['screenshot_pixels']) for r in result['rows']}) == 1
    assert len({r['visible_cards'] for r in result['rows']}) == 1
    result['statistics'] = {}
    for version in deliveries:
        result['statistics'][version] = {}
        for phase in ('fresh', 'reopen'):
            group = [r for r in result['rows'] if r['version'] == version and r['phase'] == phase]
            assert len(group) == a.pairs
            result['statistics'][version][phase] = {}
            for name in group[0]['events']:
                def stats(values):
                    ordered = sorted(values)
                    return {'count': len(values), 'median_ms': statistics.median(values),
                            'p95_ms': ordered[math.ceil(.95 * len(values)) - 1] if len(values) >= 20 else None,
                            'min_ms': min(values), 'max_ms': max(values)}
                result['statistics'][version][phase][name] = {
                    'from_main': stats([r['events'][name]['elapsed_ms'] for r in group]),
                    'after_first_ui': stats([r['after_first_ui_ms'][name] for r in group])}
    result['finished'] = True
finally:
    output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf8')
print(json.dumps(result['statistics'], ensure_ascii=False, indent=2), flush=True)
