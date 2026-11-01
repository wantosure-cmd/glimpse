"""Observe native layout hashes while a dedicated 580-video fixture learns sizes.

Generated hard links refer only to generated seeds in this owned target directory.
No user collection is opened or modified. This is not physical scroll validation.
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import shutil
import subprocess
import time
import uuid

ROOT = Path(__file__).resolve().parent.parent
p = argparse.ArgumentParser()
p.add_argument('--exe', type=Path, required=True)
p.add_argument('--prefix', default='v042')
p.add_argument('--seconds', type=int, default=40)
a = p.parse_args()
assert a.prefix.replace('-', '').isalnum() and 20 <= a.seconds <= 90
exe = a.exe.resolve(strict=True)
task = ROOT/'target'/('video-stability-owned-'+uuid.uuid4().hex)
media = task/'media'
media.mkdir(parents=True)
(task/'.generated-video-stability').write_text('Owned synthetic videos; no private sources.', encoding='ascii')
ffmpeg = ROOT/'vendor/runtime/ffmpeg.exe'
seeds = []
for i, size in enumerate(('640x360', '360x640', '480x480')):
    seed = task/f'seed-{i}.mp4'
    subprocess.run([str(ffmpeg), '-v', 'error', '-f', 'lavfi', '-i',
                    'testsrc2=size='+size+':rate=12', '-t', '2', '-an',
                    '-c:v', 'libx264', '-preset', 'ultrafast', '-crf', '28', str(seed)],
                   check=True, creationflags=subprocess.CREATE_NO_WINDOW)
    seeds.append(seed)
for i in range(580):
    os.link(seeds[i % 3], media/f'{i:04}.mp4')

def hashes():
    return {f.name: hashlib.sha256(f.read_bytes()).hexdigest() for f in media.iterdir()}

before = hashes()
metrics = ROOT/f'artifacts/{a.prefix}-video-stability-load.json'
digest = hashlib.sha256(exe.read_bytes()).hexdigest()
with (task/'stdout.log').open('wb') as stdout, (task/'stderr.log').open('wb') as stderr:
    proc = subprocess.Popen([str(exe), '--open', str(media), '--data-dir', str(task/'data'),
                             '--instance-scope', task.name, '--load-metrics', str(metrics),
                             '--quit-after', str(a.seconds * 1000)], cwd=task, stdout=stdout, stderr=stderr)
    code = proc.wait(timeout=a.seconds+30)
assert code == 0
raw = json.loads(metrics.read_text(encoding='utf8'))['rows']
rows = [r for r in raw if r['scan_complete'] and r['media'] == 580
        and r.get('layout') and r['layout']['cards'] == 580]
assert len(rows) > 20
geometry = {r['layout']['geometry'] for r in rows}
known = [r['layout']['metadata_ready'] for r in rows]
assert max(known) > min(known) + 5, 'No meaningful async metadata change observed'
assert len(geometry) == 1, 'Video metadata caused native card reflow'
assert all(abs(r['layout']['scroll_y']) < .1 for r in rows), 'Top viewport moved'
assert all(r['failures'] == 0 for r in raw)
assert max(r['visible_ready'] for r in rows) > 0
assert before == hashes()
assert digest == hashlib.sha256(exe.read_bytes()).hexdigest()
out = dict(checks_pass=True, exe_sha256=digest, exit_code=code,
           fixture=str(task), generated_videos=580, geometry_variants=len(geometry),
           metadata_ready_range=[min(known), max(known)],
           first_preview_ms=next(r['elapsed_ms'] for r in raw if r['visible_ready'] > 0),
           source_hashes_unchanged=True, observations=len(rows), metrics=str(metrics),
           scope='Native Release callbacks and layout rectangles under real asynchronous probing, fresh app cache, synthetic landscape/portrait/square H.264 hard links. Stationary top viewport, not physical input, router-share timing, or all 580 files decoded simultaneously.')
(ROOT/f'artifacts/{a.prefix}-video-stability-verification.json').write_text(
    json.dumps(out, ensure_ascii=False, indent=2)+'\n', encoding='utf8')
print(json.dumps(out, ensure_ascii=False, indent=2))
