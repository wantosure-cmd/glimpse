"""Package/source integrity only; installation and full product acceptance stay separate."""
import argparse
import hashlib
import json
import pathlib
import struct
import zipfile
from package_local import production_inputs

ROOT = pathlib.Path(__file__).resolve().parent.parent
p = argparse.ArgumentParser()
p.add_argument('directory', type=pathlib.Path)
p.add_argument('--source-evidence', type=pathlib.Path, required=True)
p.add_argument('--output', type=pathlib.Path, required=True)
a = p.parse_args()
d = a.directory.resolve(strict=True)
assert d.is_relative_to(ROOT / 'dist')


def sha(p):
    with p.open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


def read(p):
    return json.loads(p.read_text(encoding='utf-8-sig'))


m = read(d / 'delivery.json')
assert m['status'] == 'local-test-only' and m['application_license'] == 'undecided-by-owner'
assert not m['missing_license_texts'] and production_inputs() == m['production_source_sha256']
for r in m['files']:
    f = d / r['file']
    assert f.stat().st_size == r['bytes'] and sha(f) == r['sha256']
for line in (d / 'SHA256SUMS.txt').read_text().splitlines():
    h, name = line.split('  ', 1)
    assert '/' not in name and '\\' not in name and sha(d / name) == h
with zipfile.ZipFile(d / 'media-browser-portable.zip') as z:
    manifest = json.loads(z.read('media-browser-portable/package-files.json'))
    for r in manifest:
        h, count = hashlib.sha256(), 0
        with z.open('media-browser-portable/' + r['path']) as f:
            for b in iter(lambda: f.read(1024 * 1024), b''):
                h.update(b)
                count += len(b)
        assert count == r['bytes'] and h.hexdigest() == r['sha256']
    assert hashlib.sha256(z.read('media-browser-portable/media-browser.exe')).hexdigest() == m['exe_sha256']
    assert json.loads(z.read('media-browser-portable/licenses/license-text-gaps.json')) == []
with zipfile.ZipFile(d / 'media-browser-source.zip') as z:
    records = json.loads(z.read('media-browser-source/SOURCE-FILES.json'))
    for r in records:
        b = z.read('media-browser-source/' + r['file'])
        assert len(b) == r['bytes'] and hashlib.sha256(b).hexdigest() == r['sha256'] == sha(ROOT / r['file'])
installer = d / 'media-browser-setup-local-test.exe'
with installer.open('rb') as f:
    f.seek(-24, 2)
    tail = f.read(24)
    assert tail[:8] == b'MBSET001'
    count, offset = struct.unpack('<QQ', tail[8:])
    start = installer.stat().st_size - count - 24
    f.seek(start)
    payload = json.loads(f.read(count))
    assert payload['stub_bytes'] == offset
    for r in payload['files']:
        assert r['offset'] == offset
        f.seek(offset)
        h, left = hashlib.sha256(), r['bytes']
        while left:
            b = f.read(min(left, 1024 * 1024))
            assert b
            h.update(b)
            left -= len(b)
        assert h.hexdigest() == r['sha256']
        offset += r['bytes']
    assert offset == start
s = read(a.source_evidence)
assert s['checks_pass'] and s['source_zip_sha256'] == sha(d / 'media-browser-source.zip')
assert s['portable_zip_sha256'] == sha(d / 'media-browser-portable.zip')
result = {'package_bytes_verified': True, 'source_archive_check_pass': True,
          'exe_sha256': m['exe_sha256'], 'source_files': len(records), 'portable_files': len(manifest),
          'installer_payload_files': len(payload['files']), 'installation_runtime_verified': False,
          'full_acceptance_pass': False,
          'scope': 'Delivery file hashes, portable/source records, installer payload offsets/bytes and fresh-source check only. Does not assert actual installation/uninstallation, public licensing, Win11, physical input or clean offline GPU acceptance.'}
a.output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf8')
print(json.dumps(result, ensure_ascii=False, indent=2))
