"""Restore pinned official libmpv; never silently update a rolling release."""
import hashlib
import json
import pathlib
import urllib.request
import zipfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
DOWNLOADS = ROOT / 'vendor/downloads'
DOWNLOADS.mkdir(parents=True, exist_ok=True)

def fetch(url, name=None):
    path = DOWNLOADS / (name or url.rsplit('/', 1)[-1])
    if not path.exists():
        print('Downloading', url, flush=True)
        req = urllib.request.Request(url, headers={'User-Agent': 'media-browser-p0'})
        with urllib.request.urlopen(req, timeout=90) as src, path.with_suffix(path.suffix+'.part').open('wb') as out:
            while block := src.read(1024*1024):
                out.write(block)
        path.with_suffix(path.suffix+'.part').replace(path)
    return path

def record(path, url, component):
    return dict(component=component, filename=path.name, source=url,
                sha256=hashlib.sha256(path.read_bytes()).hexdigest(), bytes=path.stat().st_size)

records = json.loads((ROOT / 'vendor/p0-downloads.json').read_text(encoding='utf8'))
entry = next(r for r in records if r['component'] == 'libmpv official GPL build')
path = fetch(entry['source'], entry['filename'])
if hashlib.sha256(path.read_bytes()).hexdigest() != entry['sha256']:
    raise RuntimeError('libmpv SHA-256 mismatch; do not overwrite the lock manifest')
with zipfile.ZipFile(path) as z:
    z.extractall(ROOT / 'vendor/extracted/mpv')
print('Verified and extracted', path.name)
