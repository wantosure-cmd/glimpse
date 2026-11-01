"""Archive successes and failures separately; never treat logging Debug as Release."""
from pathlib import Path
import hashlib
import json
import shutil

ROOT = Path(__file__).resolve().parent.parent
BASE = ROOT / 'dist/0.4.10-local-test-20261022-icons/verification'
OUT = BASE / 'runtime-supplement'


def read(p):
    return json.loads(p.read_text(encoding='utf-8-sig'))


def sha(p):
    with p.open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


def copy(source, target):
    source = source.resolve(strict=True)
    assert source.is_relative_to(ROOT) and not source.is_symlink()
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, target)


def copy_evidence(folder, target):
    folder = folder.resolve(strict=True)
    assert folder.is_relative_to(ROOT / 'artifacts') or folder.is_relative_to(ROOT / 'target')
    for file in folder.rglob('*'):
        if file.is_file() and file.suffix in {'.json', '.png', '.log'}:
            assert file.resolve(strict=True).is_relative_to(folder)
            copy(file, target / file.relative_to(folder))


def main():
    baseline = read(BASE / 'index.json')
    summary = read(ROOT / 'artifacts/v0410-runtime-supplement.json')
    assert summary['evidence_verified'] and not summary['checks_pass']
    assert summary['exe_sha256'] == baseline['exe_sha256']
    for row in baseline['files']:
        assert sha(BASE / row['file']) == row['sha256']
    fmt = read(ROOT / 'artifacts/v0410-format-matrix.json')
    assert fmt['checks_pass'] and fmt['finished'] and len(fmt['rows']) == 24
    assert fmt['exe_sha256'] == baseline['exe_sha256']
    OUT.mkdir(exist_ok=True)
    for row in fmt['rows']:
        assert row['passed']
        folder = (ROOT / row['evidence']).resolve(strict=True)
        assert folder.is_relative_to(ROOT / 'artifacts/p5-format-ui')
        copy_evidence(folder, OUT / 'formats' / row['name'])
    watch = read(ROOT / 'artifacts/v0410-watch-verification.json')
    assert watch['checks_pass'] and watch['production_inputs_unchanged'] and watch['original_sources_unchanged']
    copy_evidence(Path(watch['evidence']), OUT / 'watch')
    for name in ['v0410-format-matrix.json', 'v0410-format-matrix.log', 'v0410-original-heic.json',
                 'v0410-lib-tests.log', 'v0410-lib-verification.json', 'v0410-watch-verification.json',
                 'v0410-watch.log', 'v0410-edit.log', 'v0410-edit-isolated.log', 'v0410-edit-desktop.log',
                 'v0410-edit-focus-audit.json', 'v0410-edit-audit-context.json', 'v0410-edit-audit-build.log',
                 'v0410-computer-use-check.json', 'v0410-desktop-review-process.json',
                 'v0410-desktop-normal-process.json', 'v0410-runtime-supplement.json',
                 'prepare_v0410_focus_audit.py']:
        copy(ROOT / 'artifacts' / name, OUT / 'reports' / name)
    for suffix in ['593e3e1caaf54a1ebf71dac39b860e92', 'c074565f08684360894f1803742416bb',
                   '0f726e4044f4423b9e91ee0b7001d5eb']:
        copy_evidence(ROOT / 'artifacts' / ('cloud-edit-' + suffix), OUT / 'failed-release-edit' / suffix)
    audit = read(ROOT / 'artifacts/v0410-edit-focus-audit.json')
    assert audit['production_inputs_unchanged'] and audit['diagnostic_error']
    assert any(r['focused'] is False and r['blocked'] is True for r in audit['observations'])
    copy_evidence(Path(audit['evidence']), OUT / 'logging-debug-not-release')
    source = Path(audit['source']).resolve(strict=True)
    assert source.is_relative_to(ROOT / 'target')
    for name in ['Cargo.toml', 'src/native.rs', 'src/browser_controls.rs']:
        copy(source / name, OUT / 'logging-debug-not-release/modified-source' / name)
    # Deliberately do NOT copy the private HEIC decoded-output directory.
    copy(ROOT / 'docs/v0.4.10运行验收补充.md', OUT / '运行验收补充.md')
    files = [dict(file=f.relative_to(OUT).as_posix(), bytes=f.stat().st_size, sha256=sha(f))
             for f in sorted(OUT.rglob('*')) if f.is_file() and f != OUT / 'index.json']
    index = dict(evidence_integrity_verified=True, all_checks_pass=False, full_acceptance_pass=False,
                 exe_sha256=baseline['exe_sha256'], summary=summary, files=files)
    (OUT / 'index.json').write_text(json.dumps(index, ensure_ascii=False, indent=2), encoding='utf-8')
    baseline['runtime_supplement'] = dict(index='runtime-supplement/index.json', all_checks_pass=False,
                                         note='Format, HEIC, library and watch pass; edit shortcut/rotation and desktop capture do not pass')
    baseline['baseline_checks_pass'] = baseline['checks_pass']
    baseline['checks_pass'] = False
    baseline['evidence_integrity_verified'] = True
    baseline['files'] = [dict(file=f.relative_to(BASE).as_posix(), bytes=f.stat().st_size, sha256=sha(f))
                         for f in sorted(BASE.rglob('*')) if f.is_file() and f != BASE / 'index.json']
    (BASE / 'index.json').write_text(json.dumps(baseline, ensure_ascii=False, indent=2), encoding='utf-8')
    for row in baseline['files']:
        assert sha(BASE / row['file']) == row['sha256']
    print(json.dumps(dict(evidence_integrity_verified=True, all_checks_pass=False, supplemental_files=len(files))))


if __name__ == '__main__':
    main()
