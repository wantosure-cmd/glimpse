"""Compare read-only, pinned H.264 native playback observations, not click latency."""
from pathlib import Path
import hashlib
import json
import statistics

ROOT = Path(__file__).resolve().parent.parent
rows = []
for version in ('040', '041'):
    runs = []
    digest = None
    source_digest = None
    for run in range(1, 4):
        path = ROOT / f'artifacts/v{version}-video-perf-{run}.json'
        report = json.loads(path.read_text(encoding='utf8'))
        assert report['checks_pass'] and report['finished'] and len(report['rows']) == 1
        case = report['rows'][0]
        assert case['passed'] and case['name'] == 'h264-mp4'
        phases = {row['phase']: row for row in case['diagnostic']['rows']}
        player = phases['detail_fit']['player']
        assert player['ready'] and player['mute'] == 'no' and player['first_ready_render_ms'] > 0
        assert digest is None or digest == report['exe_sha256']
        assert source_digest is None or source_digest == case['source_sha256']
        digest = report['exe_sha256']
        source_digest = case['source_sha256']
        runs.append(dict(report=path.relative_to(ROOT).as_posix(),
                         first_ready_render_ms=player['first_ready_render_ms']))
    rows.append(dict(version='0.4.' + ('0' if version == '040' else '1'),
                     exe_sha256=digest, source_sha256=source_digest, runs=runs,
                     median_ms=statistics.median(row['first_ready_render_ms'] for row in runs)))
assert rows[0]['source_sha256'] == rows[1]['source_sha256']
source = ROOT / 'test-media/format-matrix-v1/h264-mp4/sample.mp4'
assert hashlib.sha256(source.read_bytes()).hexdigest() == rows[0]['source_sha256']
out = dict(checks_pass=True, versions=rows,
           scope='Three sequential runs per version of one generated H.264/AAC file. Player load request to first ready GL render; excludes initialization before load, physical mouse and presentation latency. Audio and seek checks use the native format runner. Not a 580-video network-folder benchmark or all-format acceptance.')
(ROOT/'artifacts/v041-video-performance-comparison.json').write_text(
    json.dumps(out, ensure_ascii=False, indent=2) + '\n', encoding='utf8')
print(json.dumps(out, ensure_ascii=False, indent=2))
