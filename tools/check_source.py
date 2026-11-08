"""Source-only preflight: no runtime, media, downloads or installed application needed."""
from pathlib import Path
import hashlib
import json
import py_compile
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def main():
    cargo = tomllib.loads((ROOT / 'Cargo.toml').read_text(encoding='utf8'))
    lock = tomllib.loads((ROOT / 'Cargo.lock').read_text(encoding='utf8'))
    package = cargo['package']
    assert [p['version'] for p in lock['package'] if p['name'] == package['name']] == [package['version']]
    rows = json.loads((ROOT / 'vendor/upstream-license-sources.json').read_text(encoding='utf8'))
    for row in rows:
        path = (ROOT / row['file']).resolve(strict=True)
        assert path.is_relative_to((ROOT / 'assets/licenses').resolve())
        assert hashlib.sha256(path.read_bytes()).hexdigest() == row['sha256'], row['file']
    scripts = sorted((ROOT / 'tools').glob('*.py'))
    for path in scripts:
        py_compile.compile(str(path), doraise=True)
    print(json.dumps(dict(checks_pass=True, version=package['version'], pinned_licenses=len(rows),
                          python_scripts=len(scripts), scope='Source only; not native/runtime/release acceptance')))


if __name__ == '__main__':
    main()
