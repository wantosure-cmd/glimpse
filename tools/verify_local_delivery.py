"""Validate final ZIP contents, source completeness, payload offsets and SHA pins."""
import argparse
import hashlib
import json
import struct
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def sha(p):
    h = hashlib.sha256()
    with p.open('rb') as f:
        while b := f.read(1024 * 1024):
            h.update(b)
    return h.hexdigest()


def main():
    p = argparse.ArgumentParser()
    p.add_argument('directory', type=Path)
    p.add_argument('--installation-evidence',type=Path,default=ROOT/'artifacts/p5-installation-smoke.json')
    p.add_argument('--source-evidence',type=Path,default=ROOT/'artifacts/p5-source-build.json')
    p.add_argument('--current-evidence',type=Path,default=ROOT/'artifacts/p4-current-verification.json')
    p.add_argument('--output',type=Path,default=ROOT/'artifacts/p5-delivery-verification.json')
    args = p.parse_args()
    directory = args.directory.resolve(strict=True)
    assert directory.is_relative_to(ROOT / 'dist')
    delivery = json.loads((directory / 'delivery.json').read_text(encoding='utf-8'))
    assert delivery['status'] == 'local-test-only' and delivery['application_license'] == 'undecided-by-owner'
    assert not delivery['missing_license_texts']
    for e in delivery['files']:
        f = directory / e['file']
        assert f.stat().st_size == e['bytes'] and sha(f) == e['sha256'], f.name
    for line in (directory / 'SHA256SUMS.txt').read_text().splitlines():
        expected, name = line.split('  ', 1)
        assert '/' not in name and '\\' not in name and sha(directory / name) == expected
    zip_path = directory / 'media-browser-portable.zip'
    with zipfile.ZipFile(zip_path) as z:
        assert z.testzip() is None
        manifest = json.loads(z.read('media-browser-portable/package-files.json'))
        for e in manifest:
            data = z.read('media-browser-portable/' + e['path'])
            assert len(data) == e['bytes'] and hashlib.sha256(data).hexdigest() == e['sha256']
        archive_exe = hashlib.sha256(z.read('media-browser-portable/media-browser.exe')).hexdigest()
        assert archive_exe == delivery['exe_sha256']
        assert json.loads(z.read('media-browser-portable/licenses/license-text-gaps.json')) == []
        crates = json.loads(z.read('media-browser-portable/licenses/rust-dependencies.json'))
        assert len(crates) == delivery['rust_resolved_windows_dependencies']
        assert all(e['texts'] for e in crates)
    with zipfile.ZipFile(directory / 'media-browser-source.zip') as z:
        assert z.testzip() is None
        records = json.loads(z.read('media-browser-source/SOURCE-FILES.json'))
        paths = {e['file'] for e in records}
        for e in records:
            assert not e['file'].startswith(('target/', 'dist/', 'test-media/', 'artifacts/', 'vendor/runtime/'))
            assert not e['file'].endswith(('.log', '.pyc'))
            data = z.read('media-browser-source/' + e['file'])
            assert len(data) == e['bytes'] and hashlib.sha256(data).hexdigest() == e['sha256']
            assert sha(ROOT / e['file']) == e['sha256'], 'Source archive stale: ' + e['file']
        assert {'Cargo.toml', 'Cargo.lock', 'src/main.rs', 'src/bin/media-browser-setup.rs',
                'docs/构建说明.md', 'docs/A01-A24验收记录.md', 'tools/restore_runtime.py'} <= paths
        assert 'LICENSE' not in paths  # Owner explicitly deferred the application license.
        assert {f.relative_to(ROOT).as_posix() for f in (ROOT / 'src').rglob('*.rs')} <= paths
    installer = directory / 'media-browser-setup-local-test.exe'
    with installer.open('rb') as f:
        f.seek(-24, 2)
        tail = f.read(24)
        assert tail[:8] == b'MBSET001'
        count, stub_bytes = struct.unpack('<QQ', tail[8:])
        assert count < 4*1024*1024
        start = installer.stat().st_size - 24 - count
        f.seek(start)
        m = json.loads(f.read(count))
        assert m['stub_bytes'] == stub_bytes
        previous = stub_bytes
        for e in m['files']:
            assert e['offset'] == previous
            f.seek(e['offset'])
            left = e['bytes']
            h = hashlib.sha256()
            while left:
                b = f.read(min(left, 1024*1024))
                assert b
                h.update(b)
                left -= len(b)
            assert h.hexdigest() == e['sha256']
            previous += e['bytes']
        assert previous == start
    installation = json.loads(args.installation_evidence.read_text(encoding='utf-8'))
    assert installation['checks_pass'] and installation['installer_sha256'] == sha(installer)
    assert installation['uninstall_registry_verified'] and installation['actual_directory_junction_refused']
    assert installation['exe_sha256'] == delivery['exe_sha256']
    current = json.loads(args.current_evidence.read_text(encoding='utf-8'))
    assert current['checks_pass'] and current['exe_sha256'].lower() == delivery['exe_sha256']
    source_build = json.loads(args.source_evidence.read_text(encoding='utf-8'))
    assert source_build['checks_pass'] and source_build['source_zip_sha256'] == sha(directory / 'media-browser-source.zip')
    assert source_build['portable_zip_sha256'] == sha(zip_path)
    result = dict(checks_pass=True, directory=str(directory), exe_sha256=delivery['exe_sha256'],
                  installer_sha256=sha(installer), source_files=len(records), portable_files=len(manifest),
                  rust_dependencies=len(crates), reference_only_license_crates=delivery['reference_only_license_crates'],
                  installation=installation, scope='Local test package hashes, contents, source and native installer validation; no public license/Win11/clean-offline certification')
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf-8')
    print(json.dumps({k: v for k, v in result.items() if k != 'installation'}, ensure_ascii=False, indent=2))


if __name__ == '__main__':
    main()
