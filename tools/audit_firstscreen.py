"""Audit visible texture readiness separately from completion of full enumeration."""
from pathlib import Path
import argparse
import hashlib
import json
import math
import statistics


def find_ready_sample(samples, final, field):
    return next((sample for sample in samples
        if sample['visible_cards'] == final['visible_cards']
        and sample['visible_cards'] > 0
        and sample['visible_pending_geometry'] == 0
        and sample[field] == sample['visible_cards']
        and (final.get('visible_ids') is None
             or sample.get('visible_ids') == final['visible_ids'])), None)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    assert not args.output.exists(), 'Keep prior audits; use a new output'
    source = json.loads(args.report.read_text(encoding='utf8'))
    assert source['finished'] and source['sources_unchanged']
    rows = []
    for row in source['rows']:
        path = Path(row['metrics'])
        raw = json.loads(path.read_text(encoding='utf8'))
        assert raw['version'] == row['version']
        final = raw['rows'][-1]
        assert final['media'] == source['files'] and final['scan_complete']
        events = {}
        for field in ('visible_ready', 'visible_clear_ready'):
            match = find_ready_sample(raw['rows'], final, field)
            assert match is not None, (row['phase'], field)
            events[field] = dict(elapsed_ms=match['elapsed_ms'], media=match['media'],
                                 metadata_ready=match['metadata_ready'],
                                 scan_complete=match['scan_complete'],
                                 visible_cards=match['visible_cards'],
                                 visible_identity_verified=final.get('visible_ids') is not None)
        rows.append(dict(run=row['run'], phase=row['phase'], events=events,
                         scene={key:final.get(key) for key in ('viewport_points','pixels_per_point','native_pixels_per_point','visible_cards')},
                         metrics=str(path), metrics_sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
    stats = {}
    for phase in ('fresh', 'reopen'):
        stats[phase] = {}
        group = [row for row in rows if row['phase'] == phase]
        for field in ('visible_ready', 'visible_clear_ready'):
            values = [row['events'][field]['elapsed_ms'] for row in group]
            ordered = sorted(values)
            stats[phase][field] = dict(values_ms=values, median_ms=statistics.median(values),
                                      p95_ms=ordered[math.ceil(len(values) * .95) - 1] if len(values) >= 20 else None)
    scenes = [row['scene'] for row in rows if row['scene']['viewport_points'] is not None]
    uniform_scene = all(scene == scenes[0] for scene in scenes) if scenes else None
    result = dict(source_report=str(args.report.resolve()),
                  source_report_sha256=hashlib.sha256(args.report.read_bytes()).hexdigest(),
                  exe_sha256=source['exe_sha256'], fixture_manifest_sha256=source['fixture_manifest_sha256'],
                  checks_pass=uniform_scene is not False, uniform_scene=uniform_scene,
                  rows=rows, statistics=stats,
                  scope='Original final-state/fixture gates preserved. Readiness needs final visible card count, zero pending geometry and every visible texture ready; when native identities exist they must match final visible identities. Does not require full enumeration. Window/DPI scene checked when available. Historical missing identities/scene remain unverified. Sampled native readiness, not physical presentation; OS caches not flushed; no P95 below20 runs.')
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf8')
    print(json.dumps(stats, ensure_ascii=False, indent=2))
    assert result['checks_pass'], 'Mixed window/DPI/card-count scenes; inspect rows before accepting pooled statistics'


if __name__ == '__main__':
    main()
