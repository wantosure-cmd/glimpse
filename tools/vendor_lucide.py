"""Fetch the selected, unmodified official SVGs at a fixed Lucide commit.

Developer-only command. The application never downloads icons at runtime.
"""
from pathlib import Path
import hashlib
import json
import urllib.request

COMMIT = '20aef31db93fe4d9f398dbb15b120d99c48eef95'
NAMES = '''cpu memory-stick hard-drive star minus chevron-left chevron-right arrow-up
panel-left folder search refresh-cw settings-2 x clapperboard image keyboard link
palette rotate-cw rotate-ccw copy trash play volume-2 volume-x square-check
maximize pin pin-off chevron-down square pause maximize-2 panels-top-left
scissors captions'''.split()
ROOT = Path(__file__).resolve().parent.parent / 'assets/icons/lucide'

def main():
    ROOT.mkdir(parents=True, exist_ok=True)
    entries = []
    for name in ['LICENSE'] + [f'icons/{n}.svg' for n in NAMES]:
        url = f'https://raw.githubusercontent.com/lucide-icons/lucide/{COMMIT}/{name}'
        destination = ROOT / Path(name).name
        print(name, flush=True)
        with urllib.request.urlopen(url, timeout=30) as response:
            data = response.read()
        destination.write_bytes(data)
        entries.append(dict(file=destination.name, source=url, sha256=hashlib.sha256(data).hexdigest()))
    (ROOT / 'manifest.json').write_text(json.dumps(dict(project='Lucide', commit=COMMIT,
        license='ISC and MIT (see LICENSE)', files=entries), indent=2)+'\n', encoding='utf-8')
    print(f'Vendored {len(entries)-1} official SVG icons and full upstream license.')

if __name__ == '__main__':
    main()

