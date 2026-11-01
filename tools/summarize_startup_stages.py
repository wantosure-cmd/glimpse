"""Summarize measured stages without upgrading them to physical startup acceptance."""
import hashlib
import json
import math
import pathlib
import statistics

ROOT = pathlib.Path(__file__).resolve().parent.parent
EXE = ROOT / 'target/release/media-browser.exe'
with EXE.open('rb') as f:
    digest = hashlib.file_digest(f, 'sha256').hexdigest()


def read(name):
    return json.loads((ROOT / 'artifacts' / name).read_text(encoding='utf-8-sig'))


def stats(values):
    values = sorted(values)
    return {'count': len(values), 'median_ms': statistics.median(values),
            'p95_ms': values[math.ceil(len(values) * .95) - 1],
            'min_ms': min(values), 'max_ms': max(values)}


groups = []
for name in ('v047-startup-stages.json', 'v047-startup-prelude-stages.json',
             'v047-final-startup-stages.json', 'v047-final-default-launch-stages.json',
             'v047-current-default-launch-stages.json'):
    d = read(name)
    assert d['finished'] and len(d['rows']) == d['runs_requested'] == 20
    samples = []
    for r in d['rows']:
        assert r['exit_code'] == 0 and r['input_idle'] and r['trace']['pid'] == r['pid']
        events = r['trace']['events']
        assert len({e['stage'] for e in events}) == len(events)
        assert all(a['elapsed_ms'] <= b['elapsed_ms'] for a, b in zip(events, events[1:]))
        e = {e['stage']: e['elapsed_ms'] for e in events}
        samples.append({'first_ui_ms': r['internal_first_ui_ms'],
                        'native_setup_ms': e['creation_context_ready'] - e['before_run_native'],
                        'browser_setup_ms': e['app_creator_done'] - e['creation_context_ready'],
                        'callback_dispatch_ms': e['first_ui_entry'] - e['app_creator_done'],
                        'first_ui_build_ms': e['first_ui_complete'] - e['first_ui_entry'],
                        'first_gl_callback_ms': e.get('first_gl_paint_callback'),
                        'first_gl_after_ui_ms': e.get('first_gl_paint_callback', e['first_ui_complete']) - e['first_ui_complete']})
    g = {'report': name, 'exe_sha256': d['exe_sha256'].lower(),
         'matches_current_executable': d['exe_sha256'].lower() == digest,
         'lifetime_ms': d.get('quit_after_ms', 2000),
         'all_have_gl_callback': all(s['first_gl_callback_ms'] is not None for s in samples),
         'stages': {k: stats([s[k] for s in samples if s[k] is not None]) for k in samples[0]}}
    if name == 'v047-startup-prelude-stages.json':
        g['limitation'] = 'External prelude scan invocation closed stdout early and is invalid as a controlled successful prelude; standalone GUI traces retained, not used to establish a cause.'
    if name.startswith('v047-current'):
        assert g['matches_current_executable']
    groups.append(g)
standard = read('v047-startup20.json')
assert standard['exe_sha256'].lower() == digest and standard['runs'] == len(standard['rows']) == 20
assert all(r['exit_code'] == 0 and r['input_idle'] for r in standard['rows'])
cwd = read('v047-startup-reports.json')
assert cwd['finished'] and cwd['checks_pass'] and len(cwd['rows']) == 4
assert all(r['exe_sha256'] == digest for r in cwd['rows'] if r['name'] != 'old-control')
report = {'exe_sha256': digest, 'groups': groups,
          'standard_startup': {k: v for k, v in standard.items() if k != 'rows'},
          'working_directory_regression_pass': True,
          'startup_physical_interactivity_verified': False,
          'historical_long_tail_cause_established': False, 'full_acceptance_pass': False,
          'scope': 'Stage timing and controlled generated-CWD regression, including preserved earlier diagnostic builds. Native setup spans event-loop/window/GL/renderer construction; no individual driver or OS cause inferred. No physical visibility/interactivity, cold reboot, first-media or seek P95 acceptance.'}
(ROOT / 'artifacts/v047-startup-summary.json').write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding='utf8')
print(json.dumps({'exe_sha256': digest, 'current_default_launch_stages': groups[-1]['stages'],
                  'standard_internal_p95_ms': standard['internal_first_ui_p95_ms'],
                  'long_tail_cause_established': False}, ensure_ascii=False, indent=2))
