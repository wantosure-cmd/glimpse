"""Archive bounded 0.4.8 observations, keeping product acceptance gaps explicit."""
import hashlib
import json
import shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
D = ROOT / 'dist/0.4.8-local-test-20261022-thumbnail-drain'
OUT = D / 'verification'


def read(p):
    return json.loads(p.read_text(encoding='utf-8-sig'))


def sha(p):
    with p.open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


def copy(p, relative):
    p = p.resolve(strict=True)
    assert any(p.is_relative_to(ROOT / folder) for folder in ('artifacts', 'target', 'docs'))
    assert p.suffix in ('.json', '.log', '.png', '.md')
    target = OUT / relative
    assert target.is_relative_to(OUT)
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(p, target)


m = read(D / 'delivery.json')
assert m['application_version'] == '0.4.8'
byte_check = read(ROOT / 'artifacts/v048-package-bytes.json')
installation = read(ROOT / 'artifacts/v048-installation.json')
package = read(ROOT / 'artifacts/v048-package-verification.json')
native = read(ROOT / 'artifacts/v048-design-verification.json')
ipc = read(ROOT / 'artifacts/v048-isolated-instance.json')
perf = read(ROOT / 'artifacts/v048-thumbnail-drain-summary.json')
startup = read(ROOT / 'artifacts/v048-startup20.json')
idle = read(ROOT / 'artifacts/v048-idle30.json')
formats = read(ROOT / 'artifacts/v048-format-matrix.json')
watch = read(ROOT / 'artifacts/v048-current-watch.json')
cache = read(ROOT / 'artifacts/v048-cache-recovery.json')
index = read(ROOT / 'artifacts/v048-index-recovery.json')
interaction = read(ROOT / 'artifacts/v048-current-input-interaction-recheck.json')
navigation = read(ROOT / 'artifacts/v048-current-navigation-verification.json')
features = read(ROOT / 'artifacts/v048-current-features-verification.json')
decoder_exit = read(ROOT / 'artifacts/v048-decoder-exit-final.json')
assert all(r['exe_sha256'].lower() == m['exe_sha256'] for r in
           (byte_check, installation, package, native, ipc, startup, idle, formats, watch, cache, index,
            interaction, navigation, features, decoder_exit))
assert decoder_exit['finished'] and decoder_exit['checks_pass'] and decoder_exit['sources_unchanged']
assert len(decoder_exit['rows']) == 4
assert all(r['finished'] and r['app_exit_code'] == 0 and r['children']
           and not r['alive_at_first_observed_app_exit'] and not r['alive_after_followup']
           for r in decoder_exit['rows'])
assert all(c['terminated'] for r in decoder_exit['rows'] for c in r['children'])
assert interaction['checks_pass'] and len(interaction['rows']) == 2
assert all(r['exit_code'] == r['verifier_exit_code'] == 0 and r['source_unchanged'] for r in interaction['rows'])
assert navigation['checks_pass'] and navigation['sources_unchanged'] and navigation['source_files'] == 21
assert features['checks_pass'] and features['production_source_sha256'] == m['production_source_sha256']
assert len(features['raw']['checks']) == 17 and features['raw']['checks_pass']
assert set(features['fixture_after_sha256']) == {'01-photo.png', 'generated-fixture.marker', 'with-file/keep.txt'}
assert all(features['fixture_before_sha256'][name] == value for name, value in features['fixture_after_sha256'].items())
assert all(r['checks_pass'] for r in (watch, cache, index))
assert watch['production_source_sha256'] == m['production_source_sha256']
assert watch['original_sources_unchanged'] and watch['production_inputs_unchanged']
assert watch['anchor_error_points'] == watch['descendant_anchor_error_points'] == 0
assert cache['production_inputs_match'] and len(cache['rows']) == 10
assert all(r['source_unchanged'] and r['cache_bytes_restored'] for r in cache['rows'])
assert index['production_inputs_match'] and index['sources_unchanged'] and len(index['rows']) == 7
assert 'test result: ok. 86 passed; 0 failed; 1 ignored' in (ROOT / 'artifacts/v048-library-desktop-tests.log').read_text(encoding='utf-8-sig')
assert (ROOT / 'artifacts/v048-filesystem-tests.log').read_text(encoding='utf-8-sig').count('test result: ok. 1 passed; 0 failed; 0 ignored') == 4
assert formats['finished'] and formats['checks_pass'] and len(formats['rows']) == 24
assert perf['exe_sha256']['candidate'] == m['exe_sha256']
assert byte_check['package_bytes_verified'] and byte_check['source_archive_check_pass']
assert all(r['checks_pass'] for r in (installation, package, native, ipc))
assert installation['registered_version_verified'] and installation['expected_application_version'] == '0.4.8'
assert installation['installer_sha256'] == sha(D / 'media-browser-setup-local-test.exe')
assert perf['controlled_series_complete'] and perf['identical_common_pngs_total'] >= 320
assert 'test result: ok. 23 passed; 0 failed; 2 ignored' in (ROOT / 'artifacts/v048-main-tests.log').read_text(encoding='utf-8-sig')
scroll = [read(ROOT / f'artifacts/p2-scroll-v048-{phase}4k-summary.json') for phase in ('cold', 'revisit')]
assert all(r['exe_sha256'].lower() == m['exe_sha256'] and r['max_preview_failures'] == 0
           and r['four_k_viewport'] for r in scroll)
before, after = [read(ROOT / f'artifacts/v048-perf-fixture-{phase}.json') for phase in ('before', 'after')]
assert before['all_match'] and after['all_match'] and before['checked'] == after['checked'] == 5000
assert before['source_sha256'] == after['source_sha256'] and before['files'] == after['files']
OUT.mkdir(exist_ok=True)
for pattern in ('v048*.json', 'v048*.log', 'p2-scroll-v048*.json', 'p2-scroll-v048*.log'):
    for p in (ROOT / 'artifacts').glob(pattern):
        copy(p, p.name)
series = read(ROOT / 'artifacts/v048-firstscreen-controlled20.json')
for row in series['rows']:
    metrics = Path(row['metrics'])
    assert sha(metrics) == row['metrics_sha256']
    destination = Path('firstscreen') / str(row['comparison_pair']) / row['version'] / row['phase']
    for name in ('metrics.json', 'startup-trace.json', 'first-ui.json', 'stdout.log', 'stderr.log'):
        copy(metrics.parent / name, destination / name)
    if row['comparison_pair'] == 1:
        copy(metrics.with_suffix('.png'), destination / 'preview.png')
for row in startup['rows']:
    copy(Path(row['first_ui_evidence']), Path('startup') / str(row['run']) / 'first-ui.json')
for p in (Path(native['work']) / 'artifacts').glob('design-*.png'):
    copy(p, Path('design') / p.name)
for row in formats['rows']:
    for p in (ROOT / row['evidence']).iterdir():
        if p.is_file() and p.suffix in ('.json', '.log', '.png'):
            copy(p, Path('formats') / row['name'] / p.name)
for name, result in (('watch', watch), ('cache-recovery', cache), ('index-recovery', index)):
    base = Path(result['evidence']).resolve(strict=True)
    assert base.is_relative_to(ROOT / 'artifacts') or base.is_relative_to(ROOT / 'target')
    for p in base.rglob('*'):
        if p.is_file() and p.suffix in ('.json', '.log', '.png'):
            copy(p, Path(name) / p.relative_to(base))
extras = [(r['mode'], Path(r['evidence'])) for r in interaction['rows']]
extras += [('navigation', Path(navigation['work']) / 'artifacts'),
           ('features', Path(features['task']) / 'work/artifacts')]
for name, base in extras:
    base = base.resolve(strict=True)
    assert base.is_relative_to(ROOT / 'artifacts') or base.is_relative_to(ROOT / 'target')
    for p in base.rglob('*'):
        if p.is_file() and p.suffix in ('.json', '.log', '.png'):
            copy(p, Path('interactions') / name / p.relative_to(base))
for row in decoder_exit['rows']:
    work = Path(decoder_exit['task']) / row['name']
    for name in ('stdout.log', 'stderr.log'):
        copy(work / name, Path('decoder-exit') / row['name'] / name)
    if (work / 'native').is_dir():
        for p in (work / 'native').iterdir():
            if p.is_file() and p.suffix in ('.json', '.png'):
                copy(p, Path('decoder-exit') / row['name'] / p.name)
for name in ('v0.4.8缩略图队列验证.md', 'v0.4.8当前构建A01-A24.md', 'v0.4.8运行恢复补验.md'):
    copy(ROOT / 'docs' / name, name)
result = dict(byte_check, main_tests_passed=23, main_tests_ignored=2,
              library_desktop_tests_passed=86, library_tests_ignored=1,
              restricted_library_run_failed=6, filesystem_integration_tests_passed=4,
              native_watch_refresh_pass=True, native_cache_recovery_cases_passed=10,
              native_index_recovery_cases_passed=7,
              native_input_and_playback_pass=True, native_navigation_pass=True,
              native_extended_feature_checks_passed=17,
              decoder_normal_exit_cases_passed=4,
              decoder_descendants_observed=sum(len(r['children']) for r in decoder_exit['rows']),
              native_design_pass=True, isolated_instance_handoff_pass=True, native_format_cases_passed=24,
              installation_runtime_verified=True, installation_registry_verified=True,
              controlled_firstscreen_observations=80, identical_common_cache_pngs=perf['identical_common_pngs_total'],
              startup_internal_callback_p95_ms=startup['internal_first_ui_p95_ms'],
              idle_normalized_cpu_percent=idle['normalized_total_cpu_percent'],
              scroll_observations=scroll, overall_firstscreen_p95_improvement_proven=False,
              full_acceptance_pass=False, windows11_verified=False,
              physical_presentation_and_input_verified=False,
              scope='Current build only: queue continuation, 80 readiness observations, identical common thumbnail bytes, release startup/4K scrolling/idle, native GL design, authorized isolated IPC, actual owned installation/uninstallation and independent source/package checks. No overall firstscreen P95 improvement proven; historical tests remain historical. Physical startup/selection/seek P95, clean offline GPU, Win11, actual iPhone HDR and full product acceptance remain unverified.')
result['files'] = [{'file': str(p.relative_to(OUT)), 'bytes': p.stat().st_size, 'sha256': sha(p)}
                   for p in sorted(OUT.rglob('*')) if p.is_file() and p.name != 'index.json']
(OUT / 'index.json').write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf8')
print(json.dumps({k: v for k, v in result.items() if k not in ('files', 'scroll_observations')}, ensure_ascii=False, indent=2))
print('Evidence files:', len(result['files']))
