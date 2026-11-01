"""Create840 read-only benchmark paths using hardlinks to owned fixed samples."""
from pathlib import Path
import hashlib
import json
import os

ROOT = Path(__file__).resolve().parent.parent
output = ROOT / 'target/viewport-videos840'
sources = ROOT / 'target/convergence-fixtures/videos84'
assert (sources.parent / '.owned-fixture').is_file()
if output.exists():
    raise FileExistsError('Keep existing benchmark fixture; never replace or delete it')
files = sorted(sources.iterdir())
assert len(files) == 84 and all(p.is_file() for p in files)
output.mkdir()
(output / '.owned-fixture').write_text('Owned hardlinked repeated videos; read-only test only.', encoding='utf8')
rows = []
for repeat in range(10):
    for source in files:
        target = output / f'{repeat:02d}-{source.name}'
        os.link(source, target)
        # Exact same file object; future source verification can hash each
        # unique owned object once while still checking every path identity.
        assert os.path.samefile(source, target)
        rows.append(dict(path=target.name, source=str(source.relative_to(ROOT)), bytes=source.stat().st_size))
hashes = {}
for source in files:
    with source.open('rb') as stream:
        hashes[str(source.relative_to(ROOT))] = hashlib.file_digest(stream, 'sha256').hexdigest()
manifest = dict(scope='840 repeated paths,84 owned source file objects; hardlinks, no new video bytes; not independent originals. Never run write/delete/rotation tests on these links.',
    sources=hashes, files=rows)
(output / 'fixture-manifest.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2), encoding='utf8')
print(json.dumps(dict(root=str(output), files=len(rows), unique_objects=len(files))))
