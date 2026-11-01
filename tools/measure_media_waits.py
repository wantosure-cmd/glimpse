"""Measure native texture readiness on fixed fixtures; never claim display latency."""
from pathlib import Path
import argparse
import datetime
import hashlib
import json
import math
import statistics
import subprocess
import time
import uuid

ROOT = Path(__file__).resolve().parent.parent


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exe', type=Path, required=True)
    parser.add_argument('--label', required=True)
    parser.add_argument('--fixture', choices=['photos5000', 'videos84'], required=True)
    parser.add_argument('--runs', type=int, default=3)
    parser.add_argument('--seconds', type=int, default=8)
    args = parser.parse_args()
    if not args.label.replace('-', '').isalnum() or not 1 <= args.runs <= 30 or not 5 <= args.seconds <= 60:
        raise ValueError('Invalid benchmark arguments')
    exe = args.exe.resolve(strict=True)
    fixtures = ROOT / 'target/convergence-fixtures'
    manifest_path = fixtures / 'manifest.json'
    manifest = json.loads(manifest_path.read_text(encoding='utf8'))
    files = [row for row in manifest['files'] if row['path'].startswith(args.fixture + '/')]
    output = ROOT / 'artifacts' / (args.label + '.json')
    if output.exists():
        raise FileExistsError(output)
    work = ROOT / 'target' / ('waits-' + uuid.uuid4().hex)
    work.mkdir()
    report = dict(label=args.label, fixture=args.fixture, files=len(files), runs=args.runs,
                  seconds=args.seconds, exe=str(exe), exe_sha256=sha(exe), work=str(work),
                  fixture_manifest_sha256=sha(manifest_path), rows=[], finished=False,
                  started_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                  scope='Native texture readiness sampled every 100ms or later; fresh/reused application cache. OS caches not flushed. Repeated generated/public samples, no physical presentation/selection latency claim. P95 omitted below 20 runs.')

    def save():
        output.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding='utf8')

    def verify_sources():
        return all((fixtures / row['path']).stat().st_size == row['bytes']
                   and sha(fixtures / row['path']) == row['sha256'] for row in files)

    assert verify_sources(), 'Fixed fixture content differs'
    try:
        for run in range(args.runs):
            pair = work / str(run)
            pair.mkdir()
            for phase in ('fresh', 'reopen'):
                metrics = pair / (phase + '.json')
                trace = pair / (phase + '-startup.json')
                row = dict(run=run, phase=phase, metrics=str(metrics))
                report['rows'].append(row)
                with (pair / (phase + '.log')).open('wb') as log:
                    process = subprocess.Popen([str(exe), '--open', str(fixtures / args.fixture),
                        '--data-dir', str(pair / 'data'), '--instance-scope', work.name + str(run) + phase,
                        '--load-metrics', str(metrics), '--startup-trace', str(trace),
                        '--quit-after', str(args.seconds * 1000)], cwd=pair, stdout=log, stderr=log)
                    report['active_pid'] = process.pid
                    save()
                    row['exit_code'] = process.wait(timeout=args.seconds + 45)
                    report['active_pid'] = None
                assert row['exit_code'] == 0, row
                raw = json.loads(metrics.read_text(encoding='utf8'))
                samples = raw['rows']
                final = samples[-1]
                row.update(version=raw['version'], final=final,
                           max_failures=max(s['failures'] for s in samples))
                first = lambda predicate: next((s for s in samples if predicate(s)), None)
                events = dict(first_preview=first(lambda s: s['visible_ready'] > 0),
                              first_clear=first(lambda s: s['visible_clear_ready'] > 0),
                              scan_complete=first(lambda s: s['media'] == len(files) and s['scan_complete']))
                for key in ('visible_ready', 'visible_clear_ready'):
                    events['all_' + key] = first(lambda s: s['visible_cards'] == final['visible_cards']
                        and s['visible_cards'] > 0 and s['visible_pending_geometry'] == 0
                        and s[key] == s['visible_cards'] and s['media'] == len(files))
                row['events'] = events
                save()
                assert final['media'] == len(files) and final['scan_complete'], row
                assert row['max_failures'] == 0 and all(events.values()), row
                print(json.dumps(dict(run=run, phase=phase,
                    events_ms={k: round(v['elapsed_ms'], 2) for k, v in events.items()},
                    first_preview_metadata=events['first_preview']['metadata_ready'],
                    media=events['first_preview']['media']), ensure_ascii=False), flush=True)
        assert verify_sources() and sha(exe) == report['exe_sha256']
        report['sources_unchanged'] = True
        report['statistics'] = {}
        for phase in ('fresh', 'reopen'):
            group = [row for row in report['rows'] if row['phase'] == phase]
            report['statistics'][phase] = {}
            for key in group[0]['events']:
                values = sorted(row['events'][key]['elapsed_ms'] for row in group)
                report['statistics'][phase][key] = dict(median_ms=statistics.median(values),
                    min_ms=min(values), max_ms=max(values),
                    p95_ms=values[math.ceil(len(values) * .95) - 1] if len(values) >= 20 else None)
        report['finished'] = True
    finally:
        save()
    print(json.dumps(report['statistics'], ensure_ascii=False, indent=2), flush=True)


if __name__ == '__main__':
    main()
