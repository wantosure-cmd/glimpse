"""Archive bounded final performance evidence; never copy media or application caches."""
from pathlib import Path
import argparse
import hashlib
import json
import shutil

ROOT = Path(__file__).resolve().parent.parent
p = argparse.ArgumentParser()
p.add_argument('directory', type=Path)
a = p.parse_args()
delivery = a.directory.resolve(strict=True)
assert delivery.is_relative_to(ROOT/'dist')
manifest = json.loads((delivery/'delivery.json').read_text(encoding='utf8'))
assert manifest['application_version'] == '0.4.1'
comparison = json.loads((ROOT/'artifacts/v041-performance-comparison.json').read_text())
assert comparison['after']['exe_sha256'] == manifest['exe_sha256'] and comparison['measured_budgets_pass']
video = json.loads((ROOT/'artifacts/v041-video-performance-comparison.json').read_text())
assert video['checks_pass'] and video['versions'][1]['exe_sha256'] == manifest['exe_sha256']
dest = delivery/'verification'
dest.mkdir(exist_ok=False)
records = []

def copy(source, name):
    source = source.resolve(strict=True)
    assert source.is_relative_to(ROOT) and source.is_file()
    target = dest/name
    assert target.is_relative_to(dest) and not target.exists()
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, target)
    records.append(dict(file=target.relative_to(dest).as_posix(),
                        original=source.relative_to(ROOT).as_posix(), bytes=target.stat().st_size,
                        sha256=hashlib.sha256(target.read_bytes()).hexdigest()))

patterns = ('v040-controlled-*', 'v041-controlled-*', 'p2-scroll-v040-controlled-*',
            'p2-scroll-v041-controlled-*', 'v040-large-*', 'v041-large-*',
            'v040-video-perf-*', 'v041-video-perf-*', 'v041-video-performance-*',
            'v041-native-*', 'v041-performance-comparison.json', 'v041-main-tests.log',
            'v041-thumbnail-final-tests.log', 'v041-release.log', 'v041-setup-build.log',
            'v041-source-build.json', 'v041-source-build.log', 'v041-delivery-check.log')
files = sorted({f for pattern in patterns for f in (ROOT/'artifacts').glob(pattern) if f.is_file()})
for file in files:
    copy(file, file.name)
for version in ('040', '041'):
    series = json.loads((ROOT/f'artifacts/v{version}-large-load-series.json').read_text())
    for run in series['rows']:
        folder = Path(run['metrics']).parent
        copy(folder/'metrics.json', f'large-v{version}-run-{run["run"]}/metrics.json')
        for screenshot in folder.rglob('*.png'):
            # Cache PNGs are intentionally excluded; only captured rendered evidence.
            if 'thumbnails' not in screenshot.parts:
                copy(screenshot, f'large-v{version}-run-{run["run"]}/'+screenshot.relative_to(folder).as_posix())
    for run in range(1, 4):
        report = json.loads((ROOT/f'artifacts/v{version}-video-perf-{run}.json').read_text())
        folder = ROOT/report['rows'][0]['evidence']
        assert folder.resolve().is_relative_to(ROOT/'artifacts/p5-format-ui')
        for file in folder.iterdir():
            if file.is_file():
                copy(file, f'video-v{version}-run-{run}/'+file.name)
design = json.loads((ROOT/'artifacts/v041-native-design-verification.json').read_text())
folder = Path(design['work'])/'artifacts'
assert folder.resolve().is_relative_to(ROOT/'target')
for file in folder.glob('design-*.png'):
    copy(file, 'design/'+file.name)
index = dict(exe_sha256=manifest['exe_sha256'], full_acceptance_pass=False,
             measured_performance_budgets_pass=True, native_regressions_pass=True,
             source_archive_check_pass=True, installation_runtime_verified=False,
             installation_note='Automatic approval service quota exhausted; installation/uninstallation action was not executed, not a safety rejection. No bypass attempted.',
             windows11_verified=False, physical_input_and_presentation_verified=False,
             explorer_paste_verified=False,
             scope='Bounded final same-build evidence. Synthetic duplicate media, warm uncontrolled OS cache, native callbacks/GL and main-process resource measurements. See performance report for limitations.',
             files=records)
(dest/'index.json').write_text(json.dumps(index, ensure_ascii=False, indent=2)+'\n', encoding='utf8')
print(json.dumps(dict(files=len(records), bytes=sum(r['bytes'] for r in records),
                     directory=str(dest)), ensure_ascii=False))
