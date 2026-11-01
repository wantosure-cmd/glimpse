"""Archive bounded native UI evidence and a local before/after contact sheet."""
import argparse
import hashlib
import html
import json
import shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
p = argparse.ArgumentParser()
p.add_argument('--delivery', type=Path, required=True)
p.add_argument('--prefix', required=True)
p.add_argument('--native-prefix', help='Defaults to PREFIX-native')
p.add_argument('--before', type=Path, required=True)
a = p.parse_args()
assert a.prefix.replace('-', '').isalnum()
native_prefix = a.native_prefix or a.prefix + '-native'
assert native_prefix.replace('-', '').isalnum()
delivery = a.delivery.resolve(strict=True)
before = a.before.resolve(strict=True)
assert delivery.is_relative_to(ROOT / 'dist') and before.is_relative_to(ROOT / 'dist')
manifest = json.loads((delivery / 'delivery.json').read_text(encoding='utf8'))
evidence = json.loads((ROOT / 'artifacts' / (native_prefix + '-regressions.json')).read_text(encoding='utf8'))
assert evidence['checks_pass'] and evidence['exe_sha256'] == manifest['exe_sha256']
dest = delivery / 'verification'
dest.mkdir(exist_ok=False)
records = []


def copy(source, relative):
    source = source.resolve(strict=True)
    assert source.is_file() and source.is_relative_to(ROOT)
    target = dest / relative
    assert target.resolve().is_relative_to(dest) and not target.exists()
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, target)
    records.append(dict(file=target.relative_to(dest).as_posix(), original=source.relative_to(ROOT).as_posix(),
                        bytes=target.stat().st_size, sha256=hashlib.sha256(target.read_bytes()).hexdigest()))


for pattern in (a.prefix + '-*', 'p2-scroll-' + a.prefix + '-*'):
    for f in sorted((ROOT / 'artifacts').glob(pattern)):
        if f.is_file() and f.suffix in ('.json', '.log', '.png'):
            copy(f, f.name)

for report in sorted((ROOT / 'artifacts').glob(a.prefix + '-startup*.json')):
    startup = json.loads(report.read_text(encoding='utf-8-sig'))
    for row in startup.get('rows', []):
        source = Path(row['first_ui_evidence']).resolve(strict=True)
        assert source.is_relative_to(ROOT / 'target')
        copy(source, 'startup/' + report.stem + '/' + str(row['run']) + '-first-ui.json')

design = json.loads((ROOT / 'artifacts' / (native_prefix + '-design-verification.json')).read_text(encoding='utf8'))
folder = Path(design['work']) / 'artifacts'
assert folder.resolve().is_relative_to(ROOT / 'target')
for f in sorted(folder.glob('design-*.png')):
    copy(f, 'after/' + f.name)
for f in sorted(before.glob('design-*.png')):
    copy(f, 'before/' + f.name)

interaction = json.loads((ROOT / 'artifacts' / (native_prefix + '-interaction-recheck.json')).read_text(encoding='utf8'))
for row in interaction['rows']:
    folder = Path(row['evidence'])
    assert folder.resolve().is_relative_to(ROOT / 'artifacts')
    for f in sorted(folder.iterdir()):
        if f.is_file() and f.suffix in ('.json', '.png', '.log'):
            copy(f, 'interaction/' + row['mode'] + '/' + f.name)

formats = json.loads((ROOT / 'artifacts' / (a.prefix + '-format24.json')).read_text(encoding='utf8'))
assert formats['checks_pass'] and formats['finished'] and formats['exe_sha256'] == manifest['exe_sha256']
for row in formats['rows']:
    folder = ROOT / row['evidence']
    assert folder.resolve().is_relative_to(ROOT / 'artifacts/p5-format-ui')
    for f in sorted(folder.iterdir()):
        if f.is_file() and f.suffix in ('.json', '.png', '.log'):
            copy(f, 'formats/' + folder.name + '/' + f.name)

user_heic = ROOT / 'artifacts' / (a.prefix + '-user-heic.json')
if user_heic.is_file():
    for row in json.loads(user_heic.read_text(encoding='utf8'))['rows']:
        folder = (ROOT / row['evidence']).resolve(strict=True)
        assert folder.is_relative_to(ROOT / 'artifacts/p5-format-ui')
        for f in sorted(folder.iterdir()):
            if f.is_file() and f.suffix in ('.json', '.png', '.log'):
                copy(f, 'user-heic/' + folder.name + '/' + f.name)

features = ROOT / 'artifacts' / (a.prefix + '-desktop-features-verification.json')
if features.is_file():
    task = Path(json.loads(features.read_text(encoding='utf8'))['task']).resolve(strict=True)
    assert task.is_relative_to(ROOT / 'target') and task.name.startswith('features-owned-')
    for f in sorted((task / 'work/artifacts').glob('*')):
        if f.is_file() and f.suffix in ('.json', '.png', '.log'):
            copy(f, 'features/' + f.name)

for name in ('watch', 'gif', 'index', 'cache'):
    report = ROOT / 'artifacts' / (a.prefix + '-' + name + '.json')
    if not report.is_file():
        continue
    raw = json.loads(report.read_text(encoding='utf8'))
    folder = Path(raw['evidence']).resolve(strict=True)
    assert folder.is_relative_to(ROOT / 'target') or folder.is_relative_to(ROOT / 'artifacts')
    for f in sorted(folder.rglob('*')):
        if f.is_file() and f.suffix in ('.json', '.png', '.log'):
            assert f.resolve().is_relative_to(folder)
            copy(f, name + '/' + f.relative_to(folder).as_posix())

path_report = ROOT / 'artifacts' / (a.prefix + '-path.json')
if path_report.is_file():
    path_result = json.loads(path_report.read_text(encoding='utf8'))
    path_matrix = Path(path_result['ui_evidence']).resolve(strict=True)
    copy(path_matrix, 'paths/matrix.json')
    for row in json.loads(path_matrix.read_text(encoding='utf8'))['rows']:
        folder = (ROOT / row['evidence']).resolve(strict=True)
        assert folder.is_relative_to(ROOT / 'artifacts/p5-format-ui')
        for f in sorted(folder.iterdir()):
            if f.is_file() and f.suffix in ('.json', '.png', '.log'):
                copy(f, 'paths/' + folder.name + '/' + f.name)

names = ['library', 'sidebar', 'context-menu', 'video-restored', 'settings-2', 'scale-200']
parts = ['<!doctype html><meta charset="utf-8"><title>掠影 UI 对照</title>',
         '<style>body{background:#131417;color:#e1e2e9;font:14px system-ui;margin:24px}section{margin:28px 0}div{display:grid;grid-template-columns:1fr 1fr;gap:16px}img{width:100%;border:1px solid #313239;border-radius:8px}figcaption{color:#91939e}figure{margin:0}</style>',
         '<h1>掠影 Glimpse · 第二轮视觉对照</h1><p>左：0.4.2；右：当前交付。原生程序 GL 渲染截图，非网页原型。目录名称、媒体数量及视频时刻不同，不作为视觉改进证据。</p>']
for name in names:
    filename = 'design-' + name + '.png'
    assert all((dest / side / filename).is_file() for side in ('before', 'after'))
    parts.append('<section><h2>' + html.escape(name) + '</h2><div>' + ''.join(
        '<figure><img loading="lazy" src="' + side + '/' + filename + '"><figcaption>' + label + '</figcaption></figure>'
        for side, label in (('before', '0.4.2'), ('after', manifest['application_version']))) + '</div></section>')
(dest / 'visual-review.html').write_text('\n'.join(parts), encoding='utf8')
file = dest / 'visual-review.html'
records.append(dict(file=file.name, bytes=file.stat().st_size, sha256=hashlib.sha256(file.read_bytes()).hexdigest()))
index = dict(exe_sha256=manifest['exe_sha256'], native_regressions_pass=True,
             full_acceptance_pass=False, physical_input_and_system_dpi_verified=False, windows11_verified=False,
             scope='Current same-build controlled egui input and real GL rendering. Historical 0.4.2 screenshots are explicitly labelled. See individual reports for failures and unverified scope.',
             files=records)
for suffix, key in (('installation', 'installation_runtime_verified'), ('source-build', 'source_archive_check_pass')):
    report = ROOT / 'artifacts' / (a.prefix + '-' + suffix + '.json')
    raw = json.loads(report.read_text(encoding='utf8')) if report.is_file() else {}
    index[key] = raw.get('checks_pass', False)
    if raw.get('checks_pass'):
        if suffix == 'installation':
            assert raw['exe_sha256'].lower() == manifest['exe_sha256']
            index['installation_registry_verified'] = raw.get('registered_version_verified', False)
        else:
            assert raw['source_zip_sha256'] == hashlib.sha256((delivery / 'media-browser-source.zip').read_bytes()).hexdigest()
perf = ROOT / 'artifacts' / (a.prefix + '-performance-summary.json')
if perf.is_file():
    raw = json.loads(perf.read_text(encoding='utf8'))
    assert raw['exe_sha256'] == manifest['exe_sha256']
    index['measured_resource_and_callback_targets_pass'] = raw['evidence_valid'] and raw['all_measured_proxy_targets_met']
    index['startup_timing_target_verified'] = False
    index['startup_proxy_initial_p95_ms'] = raw['startup']['internal_first_ui_p95_ms']
(dest / 'index.json').write_text(json.dumps(index, ensure_ascii=False, indent=2), encoding='utf8')
print(json.dumps(dict(files=len(records), bytes=sum(r['bytes'] for r in records)), ensure_ascii=False))
