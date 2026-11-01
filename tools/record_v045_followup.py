"""Archive existing 0.4.5 network/Delete observations without changing its release."""
import hashlib
import json
import pathlib
import shutil

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / 'artifacts/v045-followup-20261022'
NETWORK = ROOT / 'target/v045-network-firstscreen-20261022-a'
EXPECTED = 'e0daab4fb3b8e1e59537517dabcbeec9d8dcaae9c6a4222e8dddd52b2175de7d'


def read(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))


def first(rows, condition):
    return next((r for r in rows if condition(r)), None)


result = read(NETWORK / 'result.json')
metrics = read(NETWORK / 'draw-metrics.json')
assert result['completed'] and result['exit_code'] == 0
assert result['exe_sha256'].lower() == EXPECTED
rows = metrics['rows']
assert len(rows) > 20 and all(b['elapsed_ms'] > a['elapsed_ms'] for a, b in zip(rows, rows[1:]))
preview = first(rows, lambda r: r['visible_ready'] > 0)
complete = first(rows, lambda r: r['visible_cards'] > 0 and r['visible_ready'] == r['visible_cards'] and r['visible_pending_geometry'] == 0)
assert preview and complete
trash = read(ROOT / 'artifacts/p4-trash-ui-verification.json')
run = read(ROOT / 'artifacts/p4-trash-ui-run.json')
assert trash['checks_pass'] and trash['exe_sha256'].lower() == EXPECTED
assert run['exe_sha256'].lower() == EXPECTED
assert len(run['restored']) == 6
assert {r['Path']: r['Hash'] for r in run['copies_before']} == {r['Path']: r['Hash'] for r in run['copies_after']}
OUT.mkdir(exist_ok=True)
inputs = [NETWORK / n for n in ('result.json', 'draw-metrics.json', 'draw-metrics.png', 'stdout.log', 'stderr.log')]
inputs += [ROOT / 'artifacts' / n for n in ('p4-trash-ui.json', 'p4-trash-ui-run.json', 'p4-trash-ui-verification.json', 'p4-trash-ui-stdout.log', 'p4-trash-ui-stderr.log')]
files = []
for source in inputs:
    target = OUT / source.name
    shutil.copy2(source, target)
    files.append({'name': target.name, 'bytes': target.stat().st_size, 'sha256': hashlib.sha256(target.read_bytes()).hexdigest()})
summary = {
    'exe_sha256': EXPECTED,
    'full_acceptance_pass': False,
    'network': {
        'directory': result['directory'], 'sample_runs': 1,
        'fresh_application_cache': True, 'os_and_router_caches_flushed': False,
        'first_preview': preview, 'first_observed_all_visible_ready': complete,
        'final_observation': rows[-1],
        'preview_before_full_metadata': preview['metadata_ready'] < rows[-1]['media'],
        'all_visible_ready_before_scan_finished': not complete['scan_complete'],
        'playback_start_measured': False, 'p95_verified': False,
        'scope': 'Native callback/texture readiness sampled about every 100ms or later; not physical presentation. One evolving live network library, no full-library content hashing or writes. Not a reboot/OS-cache-cold measurement.'
    },
    'delete_ui': trash,
    'isolated_instance': {
        'executed': False,
        'reason': 'Automatic approval review rejected the invocation because the user previously deferred this exact isolated-instance script. No workaround or alternate launch attempted.'
    },
    'files': files,
}
(OUT / 'summary.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding='utf-8')
print(json.dumps({'summary': str(OUT / 'summary.json'), 'first_preview_ms': preview['elapsed_ms'], 'all_visible_ready_ms': complete['elapsed_ms'], 'final_media': rows[-1]['media'], 'delete_checks_pass': trash['checks_pass']}, ensure_ascii=False, indent=2))
