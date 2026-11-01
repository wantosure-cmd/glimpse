"""Summarize opt-in native render timing; cumulative phases are never pooled."""
import argparse
import hashlib
import json
import math
from pathlib import Path


def audit(report):
    summary = json.loads(report.read_text(encoding='utf-8-sig'))
    assert summary['checks_pass'], 'Native interaction checks must pass'
    result = {'exe_sha256': summary['exe_sha256'], 'source_report': str(report),
              'scope': 'Cumulative native render call timings, 2ms histogram bounds; '
                       'not physical display FPS or physical A/V synchronization', 'modes': {}}
    for run in summary['rows']:
        assert run['exit_code'] == run['verifier_exit_code'] == 0
        assert run['source_unchanged']
        mode = run['mode']
        path = Path(run['evidence']) / f'p3-{mode}-smoke.json'
        raw = json.loads(path.read_text(encoding='utf-8-sig'))
        assert raw['error'] is None
        rows = []
        for row in raw['rows']:
            diagnostic = (row.get('player') or {}).get('render_timing') if mode == 'playback' else row.get('render_timing')
            if diagnostic is None:
                continue
            timing = diagnostic['timing']
            count = timing['calls']
            bins = timing['render_histogram_2ms']
            assert len(bins) == 32 and sum(bins) == count
            p95_upper = None
            if count >= 20:
                cumulative = 0
                for index, value in enumerate(bins):
                    cumulative += value
                    if cumulative >= math.ceil(count * .95):
                        p95_upper = (index + 1) * 2 if index < 31 else timing['render_max_ms']
                        break
            rows.append({'phase': row['phase'], 'render_p95_upper_bound_ms': p95_upper,
                         **diagnostic})
        assert rows, f'No enabled timing evidence for {mode}'
        result['modes'][mode] = {'evidence': str(path),
                               'evidence_sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
                               'rows': rows}
    assert set(result['modes']) == {'input', 'playback'}
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--interaction-report', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    result = audit(args.interaction_report)
    with args.output.open('x', encoding='utf8') as file:
        json.dump(result, file, ensure_ascii=False, indent=2)
        file.write('\n')
    print(json.dumps({'output': str(args.output), 'exe_sha256': result['exe_sha256'],
                      'phases': {k: len(v['rows']) for k, v in result['modes'].items()}}))
