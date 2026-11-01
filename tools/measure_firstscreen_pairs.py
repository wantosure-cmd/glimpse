"""Readiness proxies on a fixed generated library; no presentation/seek claims."""
import argparse
import datetime
import hashlib
import json
import math
import pathlib
import statistics
import struct
import subprocess
import time
import uuid

ROOT = pathlib.Path(__file__).resolve().parent.parent
p = argparse.ArgumentParser()
p.add_argument('--delivery', type=pathlib.Path, required=True)
p.add_argument('--pairs', type=int, default=20)
p.add_argument('--pilot', action='store_true')
p.add_argument('--output', type=pathlib.Path, required=True)
p.add_argument('--seconds', type=int, default=10)
a = p.parse_args()
assert (a.pilot and a.pairs == 1) or (not a.pilot and a.pairs == 20)
assert 5 <= a.seconds <= 30
delivery = a.delivery.resolve(strict=True)
assert delivery.is_relative_to(ROOT / 'dist')
output = a.output.resolve()
assert output.is_relative_to(ROOT / 'artifacts') and not output.exists()
exe = delivery / 'media-browser-portable/media-browser.exe'
folder = ROOT / 'target/perf-media-5000'
assert folder.resolve().is_relative_to(ROOT / 'target') and (folder / '.media-browser-fixture').is_file()


def sha(p):
    with p.open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


digest = sha(exe)
assert digest == json.loads((delivery / 'delivery.json').read_text(encoding='utf8'))['exe_sha256']
base = ROOT / 'target' / ('firstscreen-pairs-' + uuid.uuid4().hex)
base.mkdir()
(base / '.generated-firstscreen-owned').write_text('Generated library/cache diagnostics only.', encoding='ascii')
result = {'exe_sha256': digest, 'fixture': str(folder), 'task': str(base), 'finished': False,
          'pairs_requested': a.pairs, 'pilot': a.pilot, 'rows': [], 'full_acceptance_pass': False,
          'observation_seconds': a.seconds,
          'physical_presentation_verified': False, 'selection_to_firstscreen_p95_verified': False,
          'scope': 'Main-entry-to-native visible texture readiness, 100ms-or-later sampling, default window, generated repeated files. Fresh application cache per pair, same data directory reused for warm phase. OS/router caches not flushed. Complete-after-scan values require all 5000 files discovered, but not all metadata. Neither physical presentation nor selection/seek latency.'}


def first(rows, predicate):
    return next((r for r in rows if predicate(r)), None)


def percentile(values):
    s = sorted(values)
    return s[math.ceil(.95 * len(s)) - 1] if len(s) >= 20 else None


try:
    for pair in range(a.pairs):
        pair_dir = base / str(pair)
        pair_dir.mkdir()
        data = pair_dir / 'data'
        assert not data.exists()
        for phase in ('fresh', 'reopen'):
            work = pair_dir / phase
            work.mkdir()
            metrics = work / 'metrics.json'
            trace = work / 'startup-trace.json'
            first_ui = work / 'first-ui.json'
            row = {'pair': pair + 1, 'phase': phase, 'data_dir': str(data), 'metrics': str(metrics)}
            row['started_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
            result['rows'].append(row)
            print(f'Starting pair {pair + 1}/{a.pairs} {phase}', flush=True)
            args = [str(exe), '--open', str(folder), '--data-dir', str(data),
                    '--instance-scope', base.name + str(pair) + phase,
                    '--load-metrics', str(metrics), '--startup-trace', str(trace),
                    '--first-ui-report', str(first_ui), '--quit-after', str(a.seconds * 1000)]
            with (work / 'stdout.log').open('wb') as stdout, (work / 'stderr.log').open('wb') as stderr:
                proc = subprocess.Popen(args, cwd=work, stdout=stdout, stderr=stderr)
                row['pid'] = proc.pid
                try:
                    row['exit_code'] = proc.wait(timeout=40)
                except subprocess.TimeoutExpired:
                    row['state'] = 'still-live; inspect this PID before restarting'
                    raise
            assert row['exit_code'] == 0, row
            row['ended_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
            startup = json.loads(trace.read_text(encoding='utf8'))
            first_ui_value = json.loads(first_ui.read_text(encoding='utf8'))
            assert startup['pid'] == proc.pid
            row.update(startup_trace=str(trace), startup=startup, first_ui=first_ui_value)
            raw = json.loads(metrics.read_text(encoding='utf8'))['rows']
            png = metrics.with_suffix('.png').read_bytes()
            assert png[:8] == b'\x89PNG\r\n\x1a\n'
            row['screenshot_pixels'] = list(struct.unpack('>II', png[16:24]))
            assert raw and all(r['failures'] == 0 for r in raw)
            final = raw[-1]
            assert final['media'] == 5000 and final['scan_complete'] and final['visible_cards'] > 0
            count = final['visible_cards']

            def complete(r, clear=False):
                return r['visible_cards'] == count and r['visible_pending_geometry'] == 0 and r['visible_clear_ready' if clear else 'visible_ready'] == count

            events = {
                'first_preview': first(raw, lambda r: r['visible_ready'] > 0),
                'all_visible_available': first(raw, lambda r: complete(r)),
                'all_visible_clear': first(raw, lambda r: complete(r, True)),
                'all_available_after_scan': first(raw, lambda r: r['scan_complete'] and r['media'] == 5000 and complete(r)),
                'all_clear_after_scan': first(raw, lambda r: r['scan_complete'] and r['media'] == 5000 and complete(r, True)),
            }
            assert all(events.values()), row
            row.update(visible_cards=count, final=final, events=events, metrics_sha256=sha(metrics))
            row['after_first_ui_ms'] = {k: v['elapsed_ms'] - first_ui_value['internal_first_ui_ms'] for k, v in events.items()}
            if phase == 'reopen':
                assert final['cache_hits'] > 0, 'No evidence of disk thumbnail cache reuse'
            print(json.dumps({'pair': pair + 1, 'phase': phase,
                              'all_available_ms': events['all_available_after_scan']['elapsed_ms'],
                              'all_clear_ms': events['all_clear_after_scan']['elapsed_ms'],
                              'visible_cards': count}, ensure_ascii=False), flush=True)
            output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf8')
    assert sha(exe) == digest
    result['statistics'] = {}
    for phase in ('fresh', 'reopen'):
        group = [r for r in result['rows'] if r['phase'] == phase]
        metrics = {}
        for key in group[0]['events']:
            values = [r['events'][key]['elapsed_ms'] for r in group]
            metrics[key] = {'runs': len(values), 'median_ms': statistics.median(values),
                            'p95_ms': percentile(values), 'min_ms': min(values), 'max_ms': max(values)}
            elapsed = [r['after_first_ui_ms'][key] for r in group]
            metrics[key]['after_first_ui'] = {'median_ms': statistics.median(elapsed), 'p95_ms': percentile(elapsed),
                                             'min_ms': min(elapsed), 'max_ms': max(elapsed)}
        result['statistics'][phase] = metrics
    result['finished'] = True
finally:
    output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf8')
print(json.dumps(result['statistics'], ensure_ascii=False, indent=2), flush=True)
