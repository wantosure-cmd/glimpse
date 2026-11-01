"""Read-only directory comparison, isolated caches, counterbalanced version order.

Native FBO screenshots can contain private media. Keep the output local.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import time
import uuid

try:
    import psutil
except ImportError:
    psutil = None

ROOT = Path(__file__).resolve().parent.parent


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def inventory(folder):
    rows = []
    for parent, dirs, files in os.walk(folder, followlinks=False):
        dirs[:] = sorted(d for d in dirs if not (Path(parent) / d).is_symlink())
        for name in sorted(files):
            path = Path(parent) / name
            stat = path.stat()
            rows.append((str(path.relative_to(folder)), stat.st_size, stat.st_mtime_ns))
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--directory', type=Path, required=True)
    parser.add_argument('--reference', type=Path, required=True)
    parser.add_argument('--candidate', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--pairs', type=int, default=3, choices=range(1, 11))
    parser.add_argument('--seconds', type=int, default=10, choices=range(5, 61))
    args = parser.parse_args()
    folder = args.directory.resolve(strict=True)
    binaries = {key: value.resolve(strict=True) for key, value in
                [('reference', args.reference), ('candidate', args.candidate)]}
    output = args.output.resolve()
    if output.exists() or not output.is_relative_to(ROOT / 'artifacts'):
        raise ValueError('Use a new output JSON under artifacts')
    work = ROOT / 'target' / ('directory-firstscreen-' + uuid.uuid4().hex)
    work.mkdir()
    before = inventory(folder)
    report = dict(finished=False, directory=str(folder), file_count=len(before),
                  source_inventory_sha256=hashlib.sha256(json.dumps(before).encode()).hexdigest(),
                  exe_sha256={key: digest(path) for key, path in binaries.items()},
                  work=str(work), rows=[], active_pid=None,
                  scope='Native draw telemetry, not physical presentation or OS-cold boot. '
                        'Fresh app cache then reused app cache, same private directory, '
                        'counterbalanced binary order. Source size/mtime inventory checked; '
                        'not a full source content hash. Screenshots stay private in target.')

    def save():
        output.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding='utf8')

    try:
        for pair in range(args.pairs):
            order = ('reference', 'candidate') if pair % 2 == 0 else ('candidate', 'reference')
            for name in order:
                run = work / f'{pair}-{name}'
                run.mkdir()
                for phase in ('fresh', 'reopen'):
                    metrics = run / f'{phase}.json'
                    with (run / f'{phase}.log').open('wb') as stream:
                        proc = subprocess.Popen([
                            str(binaries[name]), '--open', str(folder), '--data-dir', str(run / 'data'),
                            '--instance-scope', work.name + f'-{pair}-{name}-{phase}',
                            '--load-metrics', str(metrics), '--quit-after', str(args.seconds * 1000),
                        ], cwd=run, stdout=stream, stderr=subprocess.STDOUT)
                        report['active_pid'] = proc.pid
                        save()
                        deadline = time.monotonic() + args.seconds + 30
                        resources = dict(available=psutil is not None, main_ws_peak_bytes=0,
                                         tree_ws_peak_bytes=0, maximum_live_children=0,
                                         scope='100ms sampled working sets; tree sum includes shared pages more than once')
                        while proc.poll() is None:
                            if time.monotonic() > deadline:
                                raise TimeoutError(f'Process still live: {proc.pid}; inspect before restarting')
                            if psutil is not None:
                                try:
                                    main_process = psutil.Process(proc.pid)
                                    main_ws = main_process.memory_info().rss
                                    children = main_process.children(recursive=True)
                                    tree_ws = main_ws
                                    for child in children:
                                        try:
                                            tree_ws += child.memory_info().rss
                                        except psutil.Error:
                                            pass
                                    resources['main_ws_peak_bytes'] = max(resources['main_ws_peak_bytes'], main_ws)
                                    resources['tree_ws_peak_bytes'] = max(resources['tree_ws_peak_bytes'], tree_ws)
                                    resources['maximum_live_children'] = max(resources['maximum_live_children'], len(children))
                                except psutil.Error:
                                    pass
                            time.sleep(.1)
                        code = proc.wait()
                    report['active_pid'] = None
                    row = dict(pair=pair, binary=name, phase=phase, exit_code=code,
                               metrics=str(metrics), resources=resources)
                    report['rows'].append(row)
                    save()
                    if code != 0:
                        raise RuntimeError(f'Failed run retained: {row}')
                    raw = json.loads(metrics.read_text(encoding='utf8'))
                    rows = raw['rows']
                    first = lambda predicate: next((r['elapsed_ms'] for r in rows if predicate(r)), None)
                    row.update(version=raw['version'],
                               scan_complete_ms=first(lambda r: r['media'] > 0 and r['scan_complete']),
                               first_preview_ms=first(lambda r: r['visible_ready'] > 0),
                               first_clear_ms=first(lambda r: r['visible_clear_ready'] > 0),
                               all_visible_clear_ms=first(lambda r: r['visible_cards'] > 0
                                   and r['visible_clear_ready'] >= r['visible_cards']
                                   and r['visible_pending_geometry'] == 0 and r['scan_complete']),
                               visible_cards=rows[-1]['visible_cards'], media=rows[-1]['media'],
                               failures=max(r['failures'] for r in rows), final_pipeline=rows[-1]['thumbnail_pipeline'])
                    save()
                    if row['failures'] or row['all_visible_clear_ms'] is None:
                        raise RuntimeError(f'Incomplete run retained: {row}')
                    print(json.dumps(row, ensure_ascii=False), flush=True)
        report['statistics'] = {}
        for name in binaries:
            report['statistics'][name] = {}
            for phase in ('fresh', 'reopen'):
                group = [r for r in report['rows'] if r['binary'] == name and r['phase'] == phase]
                report['statistics'][name][phase] = {
                    key: dict(median_ms=statistics.median(r[key] for r in group),
                              min_ms=min(r[key] for r in group), max_ms=max(r[key] for r in group))
                    for key in ('first_preview_ms', 'first_clear_ms', 'all_visible_clear_ms')}
        report['source_inventory_unchanged'] = inventory(folder) == before
        report['binaries_unchanged'] = all(digest(path) == report['exe_sha256'][name]
                                            for name, path in binaries.items())
        assert report['source_inventory_unchanged'] and report['binaries_unchanged']
        # Reopening starts with cached dimensions, so compare equal workloads
        # within each phase rather than requiring fresh/reopen layouts to match.
        for phase in ('fresh', 'reopen'):
            assert len({r['visible_cards'] for r in report['rows'] if r['phase'] == phase}) == 1
        report['finished'] = True
    finally:
        save()
    print(json.dumps(report['statistics'], ensure_ascii=False, indent=2))


if __name__ == '__main__':
    main()
