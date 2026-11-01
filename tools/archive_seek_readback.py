"""Collect current 0.4.9 evidence; keep native readback distinct from presentation."""
import hashlib
import json
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parent.parent
D = ROOT / 'dist/0.4.9-local-test-20261022-seek-readback'
OUT = D / 'verification'


def read(p):
    return json.loads(p.read_text(encoding='utf-8-sig'))


def sha(p):
    with p.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def copy(p, destination):
    p = p.resolve(strict=True)
    assert any(p.is_relative_to(ROOT / directory) for directory in ('artifacts', 'target', 'docs'))
    assert p.suffix in ('.json', '.log', '.md', '.png')
    target = OUT / destination
    assert target.is_relative_to(OUT)
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(p, target)


m = read(D / 'delivery.json')
assert m['application_version'] == '0.4.9'
byte_check = read(ROOT / 'artifacts/v049-package-bytes.json')
package = read(ROOT / 'artifacts/v049-package-verification.json')
installation = read(ROOT / 'artifacts/v049-installation.json')
native = read(ROOT / 'artifacts/v049-design-verification.json')
formats = read(ROOT / 'artifacts/v049-format-matrix.json')
ipc = read(ROOT / 'artifacts/v049-isolated-instance.json')
seek = read(ROOT / 'artifacts/v049-seek-release20.json')
startup = read(ROOT / 'artifacts/v049-startup20.json')
idle = read(ROOT / 'artifacts/v049-idle30.json')
interaction = read(ROOT / 'artifacts/v049-current-input-interaction-recheck.json')
navigation = read(ROOT / 'artifacts/v049-current-navigation-verification.json')
assert all(r['exe_sha256'].lower() == m['exe_sha256'] for r in
           (byte_check, package, installation, native, formats, ipc, seek, startup, idle, interaction, navigation))
assert interaction['checks_pass'] and len(interaction['rows']) == 2
assert all(r['source_unchanged'] and r['exit_code'] == r['verifier_exit_code'] == 0 for r in interaction['rows'])
assert navigation['checks_pass'] and navigation['sources_unchanged']
assert byte_check['package_bytes_verified'] and byte_check['source_archive_check_pass']
assert all(r['checks_pass'] for r in (package, installation, native, formats, ipc, seek))
assert installation['installer_sha256'] == sha(D / 'media-browser-setup-local-test.exe')
assert installation['registered_version_verified'] and installation['expected_application_version'] == '0.4.9'
assert not seek['pilot'] and seek['sources_unchanged'] and sha(Path(seek['raw'])) == seek['raw_sha256']
assert len(seek['observed']['rows']) == 20 and all(r['independent_png_frame_code_verified'] for r in seek['observed']['rows'])
assert all(abs(r['observed_frame_time'] - r['slider_target']) <= 2/30 + .0001 for r in seek['observed']['rows'])
assert formats['finished'] and len(formats['rows']) == 24
assert 'test result: ok. 23 passed; 0 failed; 2 ignored' in (ROOT / 'artifacts/v049-main-final-tests.log').read_text(encoding='utf-8-sig')
scroll = [read(ROOT / f'artifacts/p2-scroll-v049-{phase}4k-summary.json') for phase in ('cold', 'revisit')]
assert all(r['exe_sha256'].lower() == m['exe_sha256'] and r['max_preview_failures'] == 0 and r['four_k_viewport'] for r in scroll)
before, after = [read(ROOT / f'artifacts/v049-perf-fixture-{phase}.json') for phase in ('before', 'after')]
assert before['all_match'] and after['all_match'] and before['files'] == after['files']
OUT.mkdir(exist_ok=True)
for pattern in ('v049*.json', 'v049*.log', 'p2-scroll-v049*.json', 'p2-scroll-v049*.log'):
    for p in (ROOT / 'artifacts').glob(pattern):
        copy(p, p.name)
copy(Path(seek['raw']), 'seek/readback.json')
for number, row in enumerate(seek['observed']['rows']):
    copy(Path(row['screenshot']), Path('seek') / f'{number:02d}.png')
for name in ('seek-reference.json', '.seek-benchmark-owned'):
    # Source generation records are in the main report; retain JSON without the video/cache.
    if name.endswith('.json'):
        copy(Path(seek['task']) / 'media' / name, Path('seek') / name)
for name in ('stdout.log', 'stderr.log'):
    copy(Path(seek['task']) / 'work' / name, Path('seek') / name)
for row in startup['rows']:
    copy(Path(row['first_ui_evidence']), Path('startup') / str(row['run']) / 'first-ui.json')
for p in (Path(native['work']) / 'artifacts').glob('design-*.png'):
    copy(p, Path('design') / p.name)
for row in formats['rows']:
    for p in (ROOT / row['evidence']).iterdir():
        if p.is_file() and p.suffix in ('.json', '.log', '.png'):
            copy(p, Path('formats') / row['name'] / p.name)
for name, base in [(r['mode'], Path(r['evidence'])) for r in interaction['rows']] + [('navigation', Path(navigation['work']) / 'artifacts')]:
    for p in base.rglob('*'):
        if p.is_file() and p.suffix in ('.json', '.log', '.png'):
            copy(p, Path('interactions') / name / p.relative_to(base))
for name in ('v0.4.9视频seek读回诊断.md', 'v0.4.9当前构建A01-A24.md'):
    copy(ROOT / 'docs' / name, name)
result = dict(byte_check, main_tests_passed=23, main_tests_ignored=2,
              native_design_pass=True, native_format_cases_passed=24,
              native_input_playback_and_navigation_pass=True,
              installation_runtime_verified=True, installation_registry_verified=True,
              isolated_instance_handoff_pass=True, seek_readback_targets=20,
              native_gpu_readback_p95_ms=seek['native_readback_p95_ms'],
              startup_internal_callback_p95_ms=startup['internal_first_ui_p95_ms'],
              idle_normalized_cpu_percent=idle['normalized_total_cpu_percent'], scroll_observations=scroll,
              full_acceptance_pass=False, windows11_verified=False,
              physical_input_and_presentation_verified=False, physical_seek_latency_verified=False,
              scope='Current Release optional seek diagnostic, independent PNG frame codes, native design/24 formats, startup/4K scroll/idle, isolated IPC, actual owned install/uninstall, source/package byte checks. Native readback is not physical input or display presentation. Win11/clean offline GPU/cold boot/HDR/physical acceptance remain incomplete; historical regression evidence stays historical.')
result['files'] = [{'file': str(p.relative_to(OUT)), 'bytes': p.stat().st_size, 'sha256': sha(p)}
                   for p in sorted(OUT.rglob('*')) if p.is_file() and p.name != 'index.json']
(OUT / 'index.json').write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf8')
print(json.dumps({k: v for k, v in result.items() if k not in ('files', 'scroll_observations')}, ensure_ascii=False, indent=2))
print('Evidence files:', len(result['files']))
