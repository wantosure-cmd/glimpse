"""Append separately authorized isolated IPC evidence to an existing delivery."""
import argparse
import hashlib
import json
import pathlib
import shutil

ROOT = pathlib.Path(__file__).resolve().parent.parent
p = argparse.ArgumentParser()
p.add_argument('--delivery', type=pathlib.Path, required=True)
p.add_argument('--report', type=pathlib.Path, required=True)
a = p.parse_args()
delivery = a.delivery.resolve(strict=True)
report = a.report.resolve(strict=True)
assert delivery.is_relative_to(ROOT / 'dist') and report.is_relative_to(ROOT / 'artifacts')


def read(p):
    return json.loads(p.read_text(encoding='utf-8-sig'))


def sha(p):
    with p.open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


m, r = read(delivery / 'delivery.json'), read(report)
assert r['checks_pass'] and r['exe_sha256'].lower() == m['exe_sha256']
assert r['server_exit_code'] == r['client_exit_code'] == 0
assert r['server_pid'] != r['client_pid'] and r['generated_copy_hashes_unchanged']
forward = json.loads(r['client_stdout'])
assert forward['forwarded'] and forward['server_pid'] == r['server_pid']
assert r['caller_token']['restricted'] is False
out = delivery / 'verification'
index = read(out / 'index.json')
assert index['exe_sha256'] == m['exe_sha256']
shutil.copy2(report, out / report.name)
log = report.with_suffix('.log')
if log.is_file():
    shutil.copy2(log, out / log.name)
index['isolated_instance_handoff_pass'] = True
index['isolated_instance_scope'] = r['scope']
index['full_acceptance_pass'] = False
index['files'] = [{'file': str(f.relative_to(out)), 'bytes': f.stat().st_size, 'sha256': sha(f)}
                  for f in sorted(out.rglob('*')) if f.is_file() and f.name != 'index.json']
(out / 'index.json').write_text(json.dumps(index, ensure_ascii=False, indent=2), encoding='utf8')
print(json.dumps({'isolated_instance_handoff_pass': True, 'exe_sha256': m['exe_sha256'],
                  'evidence_files': len(index['files']), 'scope': r['scope']}, ensure_ascii=False))
