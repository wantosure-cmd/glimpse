"""Verify the negative recycle branch without claiming successful recycling.

The complete cleanup suite remains failed when Windows refuses recycling.
This checks source preservation and the requirement for a human confirmation.
"""
from pathlib import Path
import argparse
import hashlib
import json

ROOT = Path(__file__).resolve().parent.parent
p = argparse.ArgumentParser()
p.add_argument('--report', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
a = p.parse_args()
report_path = a.report.resolve(strict=True)
assert report_path.is_relative_to(ROOT/'artifacts')
r = json.loads(report_path.read_text(encoding='utf8'))
assert not r['checks_pass'] and r['exit_code'] == 0
assert not r['raw']['checks_pass'] and r['raw']['error'].startswith('Recycling failed: ')
trace = r['raw']['trace']
last = trace[-1]
assert last['phase'] == 15 and last['elapsed_ms'] < 30000, 'Failure did not terminate promptly'
trash = last['state']['trash']
assert trash['operation'] is None and trash['permanent_prompt'] and trash['failures'] == 6
batch = trash['last']
assert batch['submitted'] and batch['release_confirmed'] and not batch['permanent']
assert batch['success_count'] == 0 and batch['failed_count'] == 6
assert all(not row['success'] for row in batch['items'])
fixture = Path(r['task']).resolve(strict=True)
assert fixture.is_relative_to(ROOT/'target') and fixture.name.startswith('features-owned-')
media = fixture/'media'
assert (media/'generated-fixture.marker').read_text() == 'dedicated disposable generated media'
before, after = r['fixture_before_sha256'], r['fixture_after_sha256']
assert len(before) == 9 and before == after
actual = {f.relative_to(media).as_posix():hashlib.sha256(f.read_bytes()).hexdigest()
          for f in sorted(media.rglob('*')) if f.is_file()}
assert actual == before and (media/'empty/nested').is_dir()
out = dict(safety_checks_pass=True, full_cleanup_suite_pass=False,
           exe_sha256=r['exe_sha256'], report=str(report_path.relative_to(ROOT)),
           explicit_failure_ms=last['elapsed_ms'], protected_files=len(before),
           all_source_hashes_unchanged=True, manual_confirmation_required=True,
           permanent_delete_executed=False,
           scope='Negative branch only: Windows refused recycling, all dedicated generated source files remained byte-identical, and a human confirmation was pending. Does not prove successful recycle/restore, ordinary desktop behavior, or A14 as a whole.')
a.output.write_text(json.dumps(out,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(json.dumps(out,ensure_ascii=False,indent=2))
