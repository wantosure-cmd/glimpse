"""Create owned, hash-inventoried repeated media for before/after wait measurements."""
from pathlib import Path
import hashlib
import json
import shutil

ROOT = Path(__file__).resolve().parent.parent
BASE = ROOT / 'target/convergence-fixtures'
if BASE.exists():
    raise FileExistsError('Keep existing fixed fixtures; do not overwrite them')
sources = {
    'jpeg': ROOT / 'test-media/ui-demo/封面.jpg',
    'png': ROOT / 'artifacts/heic-10bit.png',
    'heic': ROOT / 'test-media/upstream-example.heic',
    'mov': ROOT / 'test-media/test-h264-aac.mov',
    '3gp': ROOT / 'test-media/format-matrix-v1/h263-amr-3gp/sample.3gp',
}
digest = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
hashes = {key: digest(path) for key, path in sources.items()}
BASE.mkdir(parents=True)
(BASE / '.owned-fixture').write_text('Repeated local test samples; never user library.', encoding='utf8')
rows = []


def copy(key, relative):
    source = sources[key]
    destination = BASE / relative
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    assert digest(destination) == hashes[key]
    rows.append(dict(path=relative, bytes=source.stat().st_size, sha256=hashes[key]))


for group in range(100):
    for n in range(50):
        key = 'jpeg' if n < 46 else 'png' if n < 49 else 'heic'
        copy(key, f'photos5000/图集 {group + 1}/{n + 1:03d}{sources[key].suffix}')
for n in range(84):
    key = 'mov' if n % 3 == 0 else '3gp'
    copy(key, f'videos84/{n + 1:03d}{sources[key].suffix}')
manifest = dict(scope='5000 repeated photos, 84 repeated videos; not independent camera originals',
                sources={key: dict(path=str(path.relative_to(ROOT)), sha256=hashes[key])
                         for key, path in sources.items()}, files=rows)
(BASE / 'manifest.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2), encoding='utf8')
print(json.dumps(dict(root=str(BASE), files=len(rows), bytes=sum(r['bytes'] for r in rows))))
