"""Audit visible-content readiness after fixed native flings, retaining failed pauses."""
from pathlib import Path
import argparse
import hashlib
import json
import math
import statistics


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def audit(native, load):
    assert native['error'] is None, native['error']
    assert native['pattern'] == 'fling_then_read'
    samples = native['samples']
    assert samples and samples[-1]['elapsed_ms'] >= native['requested_seconds'] * 1000
    assert max(s['scroll_y'] for s in samples) - min(s['scroll_y'] for s in samples) > 2, 'Fixture did not scroll; a fully visible directory cannot prove fling readiness'
    origins = [s['app_elapsed_ms'] - s['elapsed_ms'] for s in samples]
    origin = statistics.median(origins)
    assert max(origins) - min(origins) < 1, 'Native time origins diverged'
    rows = load['rows']
    pauses = []
    index = 0
    while True:
        stop = 2300 + 3000 * index
        end = 5000 + 3000 * index
        if end > native['requested_seconds'] * 1000:
            break
        segment = [s for s in samples if stop + 100 <= s['elapsed_ms'] < end - 100]
        assert segment, 'No samples in reading pause'
        target = statistics.median(s['scroll_y'] for s in segment)
        position_stable = max(s['scroll_y'] for s in segment) - min(s['scroll_y'] for s in segment) <= 2
        observed = [r for r in rows if origin + stop <= r['elapsed_ms'] < origin + end]
        assert observed, 'No load samples in reading pause'
        tail = observed[-1]
        assert 'visible_ids' in tail, 'Requires native visible identity diagnostic'
        identities = tail['visible_ids']
        valid = lambda r: (position_stable and r['visible_ids'] == identities and len(identities) > 0
            and r['visible_cards'] == len(identities) and r['visible_pending_geometry'] == 0
            and r['layout'] is not None and abs(r['layout']['scroll_y'] - target) <= 2)
        def event(field, fraction):
            found = next((r for r in observed if valid(r)
                and r[field] >= math.ceil(r['visible_cards'] * fraction)), None)
            return None if found is None else found['elapsed_ms'] - origin - stop
        first_at_target = next((r for r in observed if position_stable
            and r['layout'] is not None and abs(r['layout']['scroll_y'] - target) <= 2
            and r['visible_cards'] > 0 and r['visible_ready'] > 0), None)
        pauses.append(dict(index=index, stop_benchmark_ms=stop, target_scroll_y=target,
            position_stable=position_stable, final_visible_ids=identities,
            cards=tail['visible_cards'], pending_geometry=tail['visible_pending_geometry'],
            first_at_target_preview_ms=None if first_at_target is None else first_at_target['elapsed_ms'] - origin - stop,
            first_preview_ms=event('visible_ready', 1 / max(1, len(identities))),
            half_preview_ms=event('visible_ready', .5),
            all_preview_ms=event('visible_ready', 1),
            all_clear_ms=event('visible_clear_ready', 1),
            max_failures=max(r['failures'] for r in observed)))
        index += 1
    assert pauses
    stats = {}
    for field in ('first_at_target_preview_ms', 'first_preview_ms', 'half_preview_ms', 'all_preview_ms', 'all_clear_ms'):
        values = sorted(p[field] for p in pauses if p[field] is not None)
        stats[field] = dict(successes=len(values), misses=len(pauses) - len(values),
            median_ms=statistics.median(values) if values else None,
            max_ms=max(values) if values else None,
            p95_ms=values[math.ceil(.95 * len(values)) - 1] if len(values) >= 20 else None)
    return dict(pauses=pauses, statistics=stats,
        all_pauses_ready=all(p['all_clear_ms'] is not None and p['max_failures'] == 0 for p in pauses),
        scope='Native samples every100ms or later after18000pt/s swipe. first_at_target_preview allows partial visible geometry at stable position; other metrics require final visible identities and zero pending geometry. Misses retained; under20 samples no P95. Not physical input/display or video motion readiness.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--native', type=Path, required=True)
    parser.add_argument('--load', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        raise FileExistsError(args.output)
    native = json.loads(args.native.read_text(encoding='utf-8-sig'))
    load = json.loads(args.load.read_text(encoding='utf-8-sig'))
    result = audit(native, load)
    result.update(input_mode=native.get('input_mode'),
                  sizing_priority_scope=native.get('sizing_priority_scope'),
                  pending_geometry_scope=load.get('pending_geometry_scope', 'legacy-label-position'))
    result.update(native=str(args.native.resolve()), load=str(args.load.resolve()),
                  native_sha256=digest(args.native), load_sha256=digest(args.load))
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf8')
    print(json.dumps({k: v for k, v in result.items() if k != 'pauses'}, ensure_ascii=False))


if __name__ == '__main__':
    main()
