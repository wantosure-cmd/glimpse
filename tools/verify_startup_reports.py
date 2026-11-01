"""Actual GUI launches in marked copied working directories; no OS input injection."""
import hashlib
import json
import pathlib
import shutil
import subprocess
import uuid

ROOT = pathlib.Path(__file__).resolve().parent.parent
NEW = ROOT / 'target/release/media-browser.exe'
OLD = ROOT / 'dist/0.4.6-local-test-20261022-network-scan/media-browser-portable/media-browser.exe'
BASE = ROOT / 'target' / ('startup-reports-' + uuid.uuid4().hex)
BASE.mkdir()
(BASE / '.generated-startup-report-fixture').write_text('Generated copies only.', encoding='ascii')
REPORT = ROOT / 'artifacts/v047-startup-reports.json'
result = {'finished': False, 'checks_pass': False, 'full_acceptance_pass': False,
          'fixture': str(BASE), 'rows': [],
          'scope': 'Native GUI startup and directory browsing, generated working-directory copies, no physical input. Old control must reproduce its unwanted diagnostic output; new launches must leave every working-directory entry and byte unchanged.'}


def sha(p):
    with p.open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


def snapshot(p):
    return {f.relative_to(p).as_posix(): (sha(f) if f.is_file() else 'directory')
            for f in p.rglob('*')}


try:
    for name, exe, opened, diagnostics in (
        ('old-control', OLD, False, False),
        ('normal-empty', NEW, False, False),
        ('normal-library', NEW, True, False),
        ('explicit-reports', NEW, True, True),
    ):
        case = BASE / name
        cwd = case / '工作目录'
        cwd.mkdir(parents=True)
        shutil.copyfile(ROOT / 'test-media/format-matrix-v1/png/sample.png', cwd / '保留图片.png')
        (cwd / '保留设置.txt').write_text('Untouched generated working-directory sentinel.', encoding='utf8')
        before = snapshot(cwd)
        first = case / 'first-ui.json'
        trace = case / 'trace.json'
        args = [str(exe), '--instance-scope', BASE.name + '-' + name,
                '--data-dir', str(case / 'data'), '--quit-after', '1500']
        if opened:
            args += ['--open', str(cwd)]
        if diagnostics:
            args += ['--first-ui-report', str(first), '--startup-trace', str(trace)]
        with (case / 'stdout.log').open('wb') as stdout, (case / 'stderr.log').open('wb') as stderr:
            p = subprocess.Popen(args, cwd=cwd, stdout=stdout, stderr=stderr)
            row = {'name': name, 'exe_sha256': sha(exe), 'pid': p.pid, 'cwd': str(cwd), 'before': before}
            result['rows'].append(row)
            try:
                row['exit_code'] = p.wait(timeout=30)
            except subprocess.TimeoutExpired:
                row['state'] = 'still-live; inspect this PID before restarting'
                raise
        assert row['exit_code'] == 0, row
        row['after'] = snapshot(cwd)
        row['original_bytes_preserved'] = all(row['after'].get(k) == v for k, v in before.items())
        row['working_directory_unchanged'] = before == row['after']
        assert row['original_bytes_preserved']
        if name == 'old-control':
            assert not row['working_directory_unchanged']
            assert 'artifacts/browser-first-ui.json' in row['after']
            row['expected_old_regression_reproduced'] = True
        else:
            assert row['working_directory_unchanged'], row
            assert first.is_file() == trace.is_file() == diagnostics
            if diagnostics:
                d = json.loads(first.read_text(encoding='utf8'))
                t = json.loads(trace.read_text(encoding='utf8'))
                assert d['internal_first_ui_ms'] > 0 and t['pid'] == p.pid
                stages = {r['stage']: r['elapsed_ms'] for r in t['events']}
                assert {'first_ui_entry', 'first_ui_complete', 'first_gl_paint_callback', 'native_loop_returned'} <= stages.keys()
                assert abs(d['internal_first_ui_ms'] - stages['first_ui_preferences_ready']) < 10
                row['first_ui'] = d
                row['trace'] = t
        REPORT.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf8')
    result['finished'] = result['checks_pass'] = True
finally:
    REPORT.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf8')
print(json.dumps({'checks_pass': result['checks_pass'], 'fixture': str(BASE),
                  'new_exe_sha256': sha(NEW), 'cases': len(result['rows'])}, ensure_ascii=False, indent=2))
