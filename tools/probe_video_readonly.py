"""Bound an existing app's null-output probe to one unchanged source and exe."""
from pathlib import Path
import argparse
import hashlib
import json
import subprocess
import time

ROOT = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--exe', type=Path, required=True)
parser.add_argument('--source', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
exe, source = args.exe.resolve(strict=True), args.source.resolve(strict=True)
output = args.output.resolve()
if output.exists() or not output.is_relative_to(ROOT / 'artifacts'):
    raise ValueError('Use a new report under artifacts')


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


before_stat = source.stat()
report = dict(exe=str(exe), exe_sha256=sha(exe), source=str(source),
              source_before_sha256=sha(source), finished=False,
              scope='Native application null video/audio probe on one read-only source. Not GPU rendering, first visible frame, audio output or performance acceptance. Source hash reads warm OS/router caches before the probe.')
with output.with_suffix('.stdout.log').open('wb') as stdout, output.with_suffix('.stderr.log').open('wb') as stderr:
    began = time.monotonic()
    process = subprocess.Popen([str(exe), '--probe-video', str(source)], cwd=ROOT, stdout=stdout, stderr=stderr)
    report['pid'] = process.pid
    output.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding='utf8')
    try:
        report['exit_code'] = process.wait(timeout=65)
    except subprocess.TimeoutExpired:
        report['observation_timeout_live_pid'] = process.pid
        output.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding='utf8')
        raise
report['probe_elapsed_ms'] = (time.monotonic() - began) * 1000
report['source_after_sha256'] = sha(source)
after_stat = source.stat()
report['source_unchanged'] = (report['source_before_sha256'] == report['source_after_sha256']
    and (before_stat.st_size, before_stat.st_mtime_ns) == (after_stat.st_size, after_stat.st_mtime_ns))
report['finished'] = True
output.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding='utf8')
print(json.dumps({key: value for key, value in report.items() if key not in ('source', 'exe')}, indent=2))
assert report['source_unchanged'] and sha(exe) == report['exe_sha256']
