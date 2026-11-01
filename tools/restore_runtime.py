"""Restore pinned app-local runtime from a local, SHA-verified delivery ZIP.

No network, extractall, executable invocation, or overwrite of different files.
"""
import argparse
import hashlib
import json
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def main():
    p = argparse.ArgumentParser()
    p.add_argument('portable_zip', type=Path)
    p.add_argument('--sha256', required=True, help='Value from delivery SHA256SUMS.txt')
    args = p.parse_args()
    h = hashlib.sha256()
    with args.portable_zip.open('rb') as f:
        while b := f.read(1024 * 1024):
            h.update(b)
    if h.hexdigest().lower() != args.sha256.lower():
        raise RuntimeError('Portable ZIP SHA256 mismatch')
    with zipfile.ZipFile(args.portable_zip) as z:
        manifest = json.loads(z.read('media-browser-portable/package-files.json'))
        pinned = {e['file']: e for e in json.loads((ROOT / 'vendor/runtime-manifest.json').read_text(encoding='utf-8-sig'))}
        tools = json.loads((ROOT / 'vendor/media-tools.json').read_text(encoding='utf-8-sig'))
        for e in tools:
            pinned[e['component'] + '.exe'] = e
        entries = [e for e in manifest if e['path'].startswith('runtime/')]
        expected = {e['path'] for e in entries}
        actual = {i.filename.removeprefix('media-browser-portable/') for i in z.infolist()
                  if i.filename.startswith('media-browser-portable/runtime/') and not i.is_dir()}
        if actual != expected:
            raise RuntimeError('Runtime archive entries differ from manifest')
        pending = []
        for e in entries:
            name = e['path'].removeprefix('runtime/')
            if not name or '/' in name or '\\' in name or ':' in name or name in ('.', '..'):
                raise RuntimeError('Unsafe runtime entry')
            data = z.read('media-browser-portable/' + e['path'])
            digest = hashlib.sha256(data).hexdigest()
            if digest.lower() != e['sha256'].lower() or len(data) != e['bytes']:
                raise RuntimeError('Runtime entry hash/size mismatch: ' + name)
            if name in pinned and digest.lower() != pinned[name]['sha256'].lower():
                raise RuntimeError('Runtime entry differs from project pin: ' + name)
            dest = ROOT / 'vendor/runtime' / name
            if dest.is_symlink():
                raise RuntimeError('Refusing symlink: ' + name)
            if dest.exists() and hashlib.sha256(dest.read_bytes()).hexdigest() != digest:
                raise RuntimeError('Refusing overwrite of different runtime: ' + name)
            if not dest.exists():
                pending.append((dest, data))
        if not set(pinned).issubset({e['path'].removeprefix('runtime/') for e in entries}):
            raise RuntimeError('Portable runtime missing pinned project entries')
        (ROOT / 'vendor/runtime').mkdir(exist_ok=True)
        for dest, data in pending:
            with dest.open('xb') as f:
                f.write(data)
        print('Verified runtime files:', len(entries), '; restored:', len(pending))


if __name__ == '__main__':
    main()
