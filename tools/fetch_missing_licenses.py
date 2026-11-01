"""Restore upstream license texts absent from published crate archives.

Use the exact .cargo_vcs_info.json revision, never a mutable default branch.
The generated provenance manifest pins content for subsequent offline packaging.
"""
import concurrent.futures
import hashlib
import json
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
metadata = json.loads((ROOT / 'artifacts/p5-cargo-metadata.json').read_text(encoding='utf-8-sig'))
names = {'accesskit', 'clipboard-win', 'ecolor', 'eframe', 'egui', 'egui-wgpu',
         'egui-winit', 'egui_glow', 'emath', 'epaint', 'epaint_default_fonts',
         'gl_generator', 'khronos_api', 'profiling'}


def fetch(package):
    crate = Path(package['manifest_path']).parent
    revision = json.loads((crate / '.cargo_vcs_info.json').read_text())['git']['sha1']
    package_revision = revision
    repo = package['repository'].removeprefix('https://github.com/').split('/tree/')[0].rstrip('/')
    candidates = ['LICENSE'] if package['name'] == 'clipboard-win' else ['LICENSE-APACHE', 'LICENSE-MIT']
    if package['name'] in ('gl_generator', 'khronos_api'):
        candidates = ['LICENSE', 'LICENSE-APACHE']
    # Several 0.37.0 crate archives name a commit that upstream currently returns
    # 404 for. Preserve that fact. The eframe 0.37.0 workspace's available commit
    # provides reference license texts, not proof that the missing source matches.
    reference_only = repo == 'emilk/egui' and revision == '7c0b40402271e8d40cda69f87cf3c2617c2e4382'
    if reference_only:
        revision = '3c6eb8261c3293ad9c7ea1ed58487e038860ebef'
    records = []
    for name in candidates:
        url = f'https://raw.githubusercontent.com/{repo}/{revision}/{name}'
        try:
            with urllib.request.urlopen(url, timeout=25) as response:
                data = response.read(1024 * 1024)
            if len(data) < 200 or b'<html' in data[:200].lower():
                raise RuntimeError('Not a license text: ' + url)
        except urllib.error.HTTPError as e:
            if e.code == 404:
                continue
            raise
        dest = Path('assets/licenses/upstream') / f"{package['name']}-{package['version']}" / name
        dest_path = ROOT / dest
        dest_path.parent.mkdir(parents=True, exist_ok=True)
        dest_path.write_bytes(data)
        records.append(dict(crate=package['name'], version=package['version'], revision=revision,
                            package_revision=package_revision, reference_only=reference_only,
                            note='Package VCS commit unavailable upstream; reference from eframe 0.37.0 workspace' if reference_only else 'Exact package VCS revision',
                            file=dest.as_posix(), source=url, sha256=hashlib.sha256(data).hexdigest()))
    return package['name'], records


if __name__ == '__main__':
    packages = [p for p in metadata['packages'] if p['name'] in names]
    all_records = []
    gaps = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=5) as pool:
        for name, records in pool.map(fetch, packages):
            if not records:
                gaps.append(name)
            all_records.extend(records)
            print(name, len(records), flush=True)
    (ROOT / 'vendor/upstream-license-sources.json').write_text(
        json.dumps(all_records, indent=2) + '\n', encoding='utf-8')
    if gaps:
        raise RuntimeError('Missing: ' + ', '.join(gaps))
