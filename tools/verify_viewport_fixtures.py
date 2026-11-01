"""Read-only full hash verification of fixed samples and owned hardlink identities."""
from pathlib import Path
import argparse
import datetime
import hashlib
import json
import os

ROOT = Path(__file__).resolve().parent.parent


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    assert output.is_relative_to(ROOT / 'artifacts') and not output.exists()
    fixed = ROOT / 'target/convergence-fixtures'
    linked = ROOT / 'target/viewport-videos840'
    assert (fixed / '.owned-fixture').is_file() and (linked / '.owned-fixture').is_file()
    fixed_manifest = fixed / 'manifest.json'
    link_manifest = linked / 'fixture-manifest.json'
    fixed_rows = json.loads(fixed_manifest.read_text(encoding='utf8'))['files']
    links = json.loads(link_manifest.read_text(encoding='utf8'))
    checked = {}
    errors = []
    for row in fixed_rows:
        path = (fixed / row['path']).resolve(strict=True)
        assert path.is_relative_to(fixed)
        actual = sha(path)
        checked[path] = actual
        if path.stat().st_size != row['bytes'] or actual != row['sha256']:
            errors.append(row['path'])
    for row in links['files']:
        path = (linked / row['path']).resolve(strict=True)
        source = (ROOT / row['source']).resolve(strict=True)
        assert path.is_relative_to(linked) and source.is_relative_to(fixed / 'videos84')
        if (not os.path.samefile(path, source) or path.stat().st_size != row['bytes']
            or checked[source] != links['sources'][row['source']]):
            errors.append('linked/' + row['path'])
    result = dict(checks_pass=not errors, fixed_paths_fully_hashed=len(fixed_rows),
        hardlink_paths_identity_checked=len(links['files']), hardlink_unique_sources=len(links['sources']),
        errors=errors, ended_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
        fixed_manifest_sha256=sha(fixed_manifest), link_manifest_sha256=sha(link_manifest),
        scope='All5084 fixed paths read and SHA256 checked.840 additional paths verified as same file objects as84 fully hashed fixed sources; no source writes, no real library access.')
    output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf8')
    print(json.dumps(result, ensure_ascii=False))
    assert result['checks_pass']


if __name__ == '__main__':
    main()
