"""Validate and summarize the six alternating native runs; never substitutes for P95."""
import hashlib
import json
import pathlib
import statistics

ROOT = pathlib.Path(__file__).resolve().parent.parent
BASE = ROOT / 'target/v046-network-paired'
rows = json.loads((BASE / 'runs.json').read_text(encoding='utf-8-sig'))
assert [r['label'] for r in rows] == ['old', 'new', 'new', 'old', 'old', 'new']
expected = {'old': 'e0daab4fb3b8e1e59537517dabcbeec9d8dcaae9c6a4222e8dddd52b2175de7d',
            'new': hashlib.sha256((ROOT / 'target/release/media-browser.exe').read_bytes()).hexdigest()}
out = ROOT / 'artifacts/v046-network-comparison.json'
summary = {'app_sha256': expected, 'full_acceptance_pass': False, 'p95_verified': False,
           'physical_presentation_verified': False, 'playback_start_verified': False,
           'os_and_router_caches_flushed': False, 'fresh_application_cache_each_run': True,
           'scope': 'Six sequential 12-second native callback/texture readiness runs on a live router SMB library; default window geometry, not 4K. Three per build, not a P95 sample. No source writes or whole-library content hashing.',
           'runs': [], 'groups': {}}
for r in rows:
    result, ready, final = r['result'], r['first_all_visible'], r['last']
    assert result['completed'] and result['exit_code'] == 0
    assert result['exe_sha256'].lower() == expected[r['label']]
    assert ready and ready['visible_ready'] == ready['visible_cards'] == 16
    assert final['failures'] == 0 and final['visible_clear_ready'] == final['visible_cards'] == 16
    assert result['first_playback_ready_render_ms'] is None
    raw = pathlib.Path(r['raw']) / 'draw-metrics.json'
    assert raw.is_relative_to(BASE)
    summary['runs'].append({'label': r['label'], 'first_preview_ms': result['first_native_preview_ms'],
                            'first_all_visible_ms': ready['elapsed_ms'], 'scan_complete_at_ready': ready['scan_complete'],
                            'final_media': final['media'], 'visible_cards': ready['visible_cards'],
                            'raw': str(raw.relative_to(ROOT)), 'raw_sha256': hashlib.sha256(raw.read_bytes()).hexdigest()})
assert len({r['final_media'] for r in summary['runs']}) == 1
for label in ('old', 'new'):
    group = [r for r in summary['runs'] if r['label'] == label]
    summary['groups'][label] = {'runs': len(group),
                              'first_preview_median_ms': statistics.median(r['first_preview_ms'] for r in group),
                              'all_visible_median_ms': statistics.median(r['first_all_visible_ms'] for r in group),
                              'all_visible_min_ms': min(r['first_all_visible_ms'] for r in group),
                              'all_visible_max_ms': max(r['first_all_visible_ms'] for r in group)}
summary['all_visible_median_reduction_percent'] = 100 * (1 - summary['groups']['new']['all_visible_median_ms'] / summary['groups']['old']['all_visible_median_ms'])
out.write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding='utf8')
print(json.dumps(summary['groups'], indent=2))
print('Median reduction percent:', summary['all_visible_median_reduction_percent'])
