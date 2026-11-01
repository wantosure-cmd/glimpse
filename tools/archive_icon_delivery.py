"""Collect current 0.4.10 evidence with explicit scope and byte hashes."""
from pathlib import Path
import hashlib
import json
import shutil

ROOT = Path(__file__).resolve().parent.parent
DELIVERY = ROOT / 'dist/0.4.10-local-test-20261022-icons'
OUT = DELIVERY / 'verification'


def read(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    manifest = read(DELIVERY / 'delivery.json')
    app = manifest['exe_sha256']
    assert sha(DELIVERY / 'media-browser-portable/media-browser.exe') == app
    icons = ROOT / 'assets/icons/lucide'
    icon_manifest = read(icons / 'manifest.json')
    assert icon_manifest['commit'] == '20aef31db93fe4d9f398dbb15b120d99c48eef95'
    assert sum(row['file'].endswith('.svg') for row in icon_manifest['files']) == 37
    for row in icon_manifest['files']:
        assert sha(icons / row['file']) == row['sha256'], row['file']
    assert {f.name for f in icons.iterdir()} == {'manifest.json'} | {r['file'] for r in icon_manifest['files']}
    evidence = {
        'design': 'v0410-reviewed-design-verification.json',
        'navigation': 'v0410-reviewed-navigation-verification.json',
        'input': 'v0410-reviewed-input-interaction-recheck.json',
        'features': 'v0410-reviewed-features-verification.json',
        'visual': 'v0410-visual-review.json',
        'installation': 'v0410-installation.json',
        'instance': 'v0410-isolated-instance.json',
        'source': 'v0410-source-build.json',
        'package_bytes': 'v0410-package-bytes.json',
        'package': 'v0410-package-verification.json',
    }
    results = {name: read(ROOT / 'artifacts' / file) for name, file in evidence.items()}
    for name, result in results.items():
        if name == 'package_bytes':
            assert result['package_bytes_verified'] and result['source_archive_check_pass'], name
        else:
            assert result['checks_pass'], name
        if 'exe_sha256' in result:
            assert result['exe_sha256'].lower() == app, name
    assert results['source']['source_zip_sha256'] == sha(DELIVERY / 'media-browser-source.zip')
    assert results['installation']['installer_sha256'] == sha(DELIVERY / 'media-browser-setup-local-test.exe')
    startup = read(ROOT / 'artifacts/v0410-startup20.json')
    assert startup['runs'] == 20 and startup['exe_sha256'].lower() == app
    for label in ['cold4k', 'revisit4k']:
        run = read(ROOT / f'artifacts/p2-scroll-v0410-{label}-summary.json')
        assert run['exe_sha256'].lower() == app and run['media'] == 5000
        assert run['seconds'] >= 119 and run['max_preview_failures'] == 0
    OUT.mkdir(exist_ok=True)

    def copy_file(source, destination):
        source = source.resolve(strict=True)
        assert source.is_relative_to(ROOT) and not source.is_symlink()
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, destination)

    patterns = ['v0410-reviewed-*', 'v0410-revised-*.log', 'v0410-catalog.log',
                'v0410-visual-review.json', 'v0410-installation.*', 'v0410-isolated-instance.*',
                'v0410-source-build.*', 'v0410-package-bytes.*', 'v0410-package-verification.*',
                'v0410-performance*', 'v0410-startup20.json', 'v0410-idle30.json',
                'v0410-perf-fixture-*.json', 'v0410-scan*', 'p2-scroll-v0410-*']
    for pattern in patterns:
        for source in (ROOT / 'artifacts').glob(pattern):
            if source.is_file():
                copy_file(source, OUT / 'current' / source.name)

    def copy_tree(source, category):
        source = source.resolve(strict=True)
        assert source.is_relative_to(ROOT / 'target') or source.is_relative_to(ROOT / 'artifacts')
        for file in source.rglob('*'):
            if file.is_file() and file.suffix.lower() in {'.png', '.json', '.log'}:
                assert file.resolve(strict=True).is_relative_to(source)
                copy_file(file, OUT / category / file.relative_to(source))

    copy_tree(Path(results['design']['work']) / 'artifacts', 'screenshots/design')
    copy_tree(Path(results['navigation']['work']) / 'artifacts', 'screenshots/navigation')
    copy_tree(Path(results['features']['task']) / 'work/artifacts', 'screenshots/features')
    for row in results['input']['rows']:
        copy_tree(Path(row['evidence']), 'input/' + row['mode'])
    old = read(ROOT / 'artifacts/v049-design-verification.json')
    assert old['exe_sha256'].lower() == '05b62e11b46e42a1ee2a14b61311aaa896d7ae00f8188a86eabce7c0d21aa07b'
    for name in ['library', 'context-menu', 'settings-1', 'video-menu']:
        copy_file(Path(old['work']) / f'artifacts/design-{name}.png', OUT / f'before-0.4.9/design-{name}.png')
    for file in [ROOT / 'docs/v0.4.10图标统一整改.md', ROOT / 'docs/v0.4.10当前构建A01-A24.md',
                 ROOT / 'docs/掠影Glimpse名称初步检索-20261022.md', ROOT / 'assets/icons/lucide/manifest.json',
                 ROOT / 'assets/icons/lucide/LICENSE']:
        copy_file(file, OUT / 'documents' / file.name)
    files = [dict(file=f.relative_to(OUT).as_posix(), bytes=f.stat().st_size, sha256=sha(f))
             for f in sorted(OUT.rglob('*')) if f.is_file() and f != OUT / 'index.json']
    index = dict(version='0.4.10', exe_sha256=app, checks_pass=True, full_acceptance_pass=False,
                 scope='Current native GPU/icon/header/folder input regressions, local packaging and performance proxies; before-0.4.9 is visual reference only',
                 limits=['No physical input/presentation or system DPI proof', 'No Win11 or clean offline OS validation',
                         'Name search is preliminary; trademark records not cleared'], evidence=evidence, files=files)
    (OUT / 'index.json').write_text(json.dumps(index, ensure_ascii=False, indent=2), encoding='utf-8')
    for row in files:
        file = OUT / row['file']
        assert file.stat().st_size == row['bytes'] and sha(file) == row['sha256']
    print(json.dumps(dict(checks_pass=True, evidence_files=len(files), exe_sha256=app)))


if __name__ == '__main__':
    main()
