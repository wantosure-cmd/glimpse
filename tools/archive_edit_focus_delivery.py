"""Bind 0.4.11 native checks to delivered bytes without claiming physical acceptance."""
from pathlib import Path
import hashlib
import json
import shutil

ROOT = Path(__file__).resolve().parent.parent
DELIVERY = ROOT / 'dist/0.4.11-local-test-20261022-edit-focus'
OUT = DELIVERY / 'verification'


def read(p):
    return json.loads(p.read_text(encoding='utf-8-sig'))


def sha(p):
    with p.open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


def main():
    delivery = read(DELIVERY / 'delivery.json')
    app = delivery['exe_sha256']
    assert delivery['application_version'] == '0.4.11'
    assert sha(DELIVERY / 'media-browser-portable/media-browser.exe') == app
    names = ['design-verification', 'navigation-verification', 'input-interaction-recheck', 'features-verification',
             'delivered-edit-controlled', 'delivered-edit-observed', 'trash-ui-verification',
             'lib-verification', 'original-heic', 'watch-verification', 'installation', 'isolated-instance', 'source-build',
             'package-verification']
    results = {n: read(ROOT / 'artifacts' / f'v0411-{n}.json') for n in names}
    for name, r in results.items():
        assert r['checks_pass'], name
        if 'exe_sha256' in r:
            assert r['exe_sha256'].lower() == app, name
    b = read(ROOT / 'artifacts/v0411-package-bytes.json')
    assert b['package_bytes_verified'] and b['source_archive_check_pass']
    assert results['source-build']['source_zip_sha256'] == sha(DELIVERY / 'media-browser-source.zip')
    fmt = read(ROOT / 'artifacts/v0411-format-matrix.json')
    assert fmt['checks_pass'] and fmt['finished'] and len(fmt['rows']) == 24
    assert fmt['exe_sha256'] == app
    for mode, expected in [('controlled', 'controlled_application_input'), ('observed', 'observed_os_focus')]:
        r = results['delivered-edit-' + mode]
        assert r['focus_mode'] == expected
        assert r['checks']['formats_preserved_and_originals_unchanged']
        if mode == 'controlled':
            assert r['checks']['background_next_and_previous_blocked']
            assert [(e['key'], e['applied_input_focus'], e['controlled'])
                    for e in r['checks']['recorded_input_context']] == [
                        ('PageDown', False, True), ('PageDown', True, True),
                        ('PageUp', False, True), ('PageUp', True, True)]
        else:
            assert [e['key'] for e in r['checks']['recorded_input_context']] == ['PageDown', 'PageUp']
            assert all(not e['controlled'] and e['observed_os_focus'] and e['applied_input_focus']
                       for e in r['checks']['recorded_input_context'])
    startup = read(ROOT / 'artifacts/v0411-startup20.json')
    assert startup['runs'] == 20 and startup['exe_sha256'].lower() == app
    for label in ['cold4k', 'revisit4k']:
        r = read(ROOT / f'artifacts/p2-scroll-v0411-{label}-summary.json')
        assert r['exe_sha256'].lower() == app and r['media'] == 5000 and r['seconds'] >= 119
        assert r['max_preview_failures'] == 0
    OUT.mkdir(exist_ok=True)

    def copy(source, target):
        source = source.resolve(strict=True)
        assert source.is_relative_to(ROOT) and not source.is_symlink()
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)

    def tree(source, target):
        source = source.resolve(strict=True)
        assert source.is_relative_to(ROOT / 'artifacts') or source.is_relative_to(ROOT / 'target')
        for f in source.rglob('*'):
            if f.is_file() and f.suffix in {'.json', '.png', '.log'}:
                assert f.resolve(strict=True).is_relative_to(source)
                copy(f, target / f.relative_to(source))

    patterns = ['v0411-design-*', 'v0411-navigation-*', 'v0411-input-*',
                'v0411-delivered-edit-*', 'v0411-trash-ui*', 'v0411-lib-*', 'v0411-watch*', 'v0411-features*',
                'v0411-main-tests.log', 'v0411-release.log', 'v0411-setup-build.log', 'v0411-catalog.log', 'v0411-original-heic.json',
                'v0411-installation.*', 'v0411-isolated-instance.*', 'v0411-source-build.*',
                'v0411-package-bytes.*', 'v0411-package-verification.*', 'v0411-format-matrix.*',
                'v0411-performance*', 'v0411-startup20.json', 'v0411-idle30.json',
                'v0411-perf-fixture-*.json', 'v0411-scan*', 'p2-scroll-v0411-*']
    for pattern in patterns:
        for f in (ROOT / 'artifacts').glob(pattern):
            if f.is_file():
                copy(f, OUT / 'current' / f.name)
    tree(Path(results['design-verification']['work']) / 'artifacts', OUT / 'design')
    tree(Path(results['navigation-verification']['work']) / 'artifacts', OUT / 'navigation')
    tree(Path(results['watch-verification']['evidence']), OUT / 'watch')
    tree(Path(results['features-verification']['task']) / 'work/artifacts', OUT / 'features')
    for mode in ['controlled', 'observed']:
        tree(Path(results['delivered-edit-' + mode]['evidence']), OUT / ('edit-' + mode))
    for row in results['input-interaction-recheck']['rows']:
        tree(Path(row['evidence']), OUT / 'input' / row['mode'])
    for row in fmt['rows']:
        assert row['passed']
        p = (ROOT / row['evidence']).resolve(strict=True)
        assert p.is_relative_to(ROOT / 'artifacts/p5-format-ui')
        tree(p, OUT / 'formats' / row['name'])
    for f in (ROOT / 'artifacts/v0411-trash-ui-evidence').glob('*'):
        if f.is_file():
            copy(f, OUT / 'trash' / f.name)
    for file in ['docs/v0.4.11快捷键与旋转诊断.md', 'docs/v0.4.11当前构建A01-A24.md',
                 'assets/icons/lucide/LICENSE', 'assets/icons/lucide/manifest.json']:
        copy(ROOT / file, OUT / 'documents' / Path(file).name)
    old = ROOT / 'dist/0.4.10-local-test-20261022-icons/verification/runtime-supplement/reports'
    for name in ['v0410-runtime-supplement.json', 'v0410-computer-use-check.json',
                 'v0410-edit-focus-audit.json', 'v0410-edit.log', 'v0410-edit-isolated.log', 'v0410-edit-desktop.log']:
        copy(old / name, OUT / 'historical-0.4.10-not-current' / name)
    files = [dict(file=f.relative_to(OUT).as_posix(), bytes=f.stat().st_size, sha256=sha(f))
             for f in sorted(OUT.rglob('*')) if f.is_file() and f != OUT / 'index.json']
    index = dict(version='0.4.11', checks_pass=True, full_acceptance_pass=False,
                 exe_sha256=app, files=files, physical_keyboard_verified=False,
                 scope='Current native app-input focus contexts, filesystem saves/recycle, GPU framebuffers, format and local-package checks; physical input/presentation/Win11/clean-offline acceptance remains open')
    (OUT / 'index.json').write_text(json.dumps(index, ensure_ascii=False, indent=2), encoding='utf-8')
    for row in files:
        assert sha(OUT / row['file']) == row['sha256']
    print(json.dumps(dict(checks_pass=True, evidence_files=len(files), exe_sha256=app, full_acceptance_pass=False)))


if __name__ == '__main__':
    main()
