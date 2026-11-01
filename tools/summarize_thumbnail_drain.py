"""Bind the complete controlled series and unchanged thumbnail bytes to both builds."""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def read(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


source = ROOT / 'artifacts/v048-firstscreen-controlled20.json'
report = read(source)
assert report['finished'] and report['pairs_requested'] == 20 and len(report['rows']) == 80
assert all(read(Path(r['path']))['finished'] and sha(Path(r['path'])) == r['sha256']
           for r in report['child_reports'])
checks = []
for pair in range(1, 21):
    paths = {}
    for version in ('reference', 'candidate'):
        row = next(r for r in report['rows'] if r['comparison_pair'] == pair
                   and r['version'] == version and r['phase'] == 'reopen')
        directory = Path(row['data_dir']).resolve(strict=True) / 'thumbnails'
        assert directory.is_relative_to(ROOT / 'target') and not directory.is_symlink()
        paths[version] = {str(p.relative_to(directory)): p for p in directory.rglob('*.png') if p.is_file()}
    common = sorted(paths['reference'].keys() & paths['candidate'].keys())
    assert len(common) >= 16, (pair, len(common))
    rows = [{'name': name, 'sha256': sha(paths['reference'][name])} for name in common]
    assert all(sha(paths['candidate'][r['name']]) == r['sha256'] for r in rows)
    checks.append({'pair': pair, 'reference_files': len(paths['reference']),
                   'candidate_files': len(paths['candidate']), 'identical_common_pngs': rows})

comparisons = {}
for phase in ('fresh', 'reopen'):
    comparisons[phase] = {}
    for metric in ('all_available_after_scan', 'all_clear_after_scan'):
        values = {}
        for origin in ('from_main', 'after_first_ui'):
            old = report['statistics']['reference'][phase][metric][origin]
            new = report['statistics']['candidate'][phase][metric][origin]
            assert old['count'] == new['count'] == 20
            values[origin] = {'reference': old, 'candidate': new,
                              'p95_reduction_percent': 100 * (old['p95_ms'] - new['p95_ms']) / old['p95_ms']}
        comparisons[phase][metric] = values

result = {'controlled_series_complete': True, 'exe_sha256': report['exe_sha256'],
          'series_sha256': sha(source), 'comparisons': comparisons, 'cache_pixel_checks': checks,
          'identical_common_pngs_total': sum(len(r['identical_common_pngs']) for r in checks),
          'full_acceptance_pass': False, 'physical_presentation_verified': False,
          'scope': 'All 80 controlled observations retained. PNG byte equality for all common disk cache keys across each pair; differing prefetch counts permitted. Texture readiness sampled at 100ms or later is neither actual display presentation nor selection-to-first-screen latency. No claim about network libraries, seek latency, cold boot or HDR.'}
(ROOT / 'artifacts/v048-thumbnail-drain-summary.json').write_text(
    json.dumps(result, ensure_ascii=False, indent=2), encoding='utf8')
print(json.dumps({k: v for k, v in result.items() if k != 'cache_pixel_checks'}, ensure_ascii=False, indent=2))
