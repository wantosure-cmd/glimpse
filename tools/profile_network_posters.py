"""Read-only, bounded stage profiling of eight network video posters, not a UI benchmark."""
import hashlib
import json
import pathlib
import subprocess
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
LIBRARY = pathlib.Path(r'\\192.168.31.1\XiaoMi-NVMe(b520)\XiaomiCamera_00_E0A25A2763A2')
APP = ROOT / 'dist/0.4.5-local-test-20261022-anchor-install/media-browser-portable/media-browser.exe'
FFMPEG = APP.parent / 'runtime/ffmpeg.exe'
OUT = ROOT / 'artifacts/v045-network-poster-stages'
EXPECTED = 'e0daab4fb3b8e1e59537517dabcbeec9d8dcaae9c6a4222e8dddd52b2175de7d'


def execute(args, timeout=65):
    start = time.perf_counter()
    p = subprocess.run([str(x) for x in args], stdin=subprocess.DEVNULL,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=timeout,
                       creationflags=subprocess.CREATE_NO_WINDOW)
    ms = (time.perf_counter() - start) * 1000
    if p.returncode:
        raise RuntimeError(p.stderr[:1000].decode('utf8', errors='replace'))
    return ms, p.stdout


def main():
    assert hashlib.sha256(APP.read_bytes()).hexdigest() == EXPECTED
    OUT.mkdir(exist_ok=False)
    (OUT / '.generated-profile-owned').write_text('Generated poster profiling outputs only.', encoding='utf8')
    report = {'exe_sha256': EXPECTED, 'library': str(LIBRARY), 'rows': [],
              'scope': 'Eight filename-sorted MP4 sources, sequential stage profiling; same streaming extraction options as production for fragmented durationless MP4; not end-to-end first-screen or playback timing. No source writes. OS/router caches not flushed.',
              'error': None}
    try:
        sources = sorted((p for p in LIBRARY.iterdir() if p.suffix.lower() == '.mp4'), key=lambda p: p.name.casefold())[:8]
        assert len(sources) == 8
        for i, source in enumerate(sources):
            before = source.stat()
            external, raw = execute([APP, '--probe-video-header', source])
            header = json.loads(raw)
            h = header['header']
            row = {'name': source.name, 'header_external_ms': external, 'header': header}
            report['rows'].append(row)
            if not h or not h['fragmented'] or h['duration_ms'] is not None:
                row['poster_skipped'] = 'Not the fragmented durationless streaming branch; no substitute measurement.'
                continue
            frame = OUT / f'{i:02d}.png'
            args = [FFMPEG, '-v', 'error', '-nostdin', '-y', '-protocol_whitelist', 'file,pipe',
                    '-seekable', '0', '-analyzeduration', '1000000', '-probesize', '1048576',
                    '-fpsprobesize', '0', '-threads', '2', '-i', source,
                    '-frames:v', '1', '-vf', 'scale=512:512:force_original_aspect_ratio=decrease:reset_sar=1',
                    '-threads', '1', frame]
            row['poster_process_ms'], _ = execute(args)
            row['poster_png_sha256'] = hashlib.sha256(frame.read_bytes()).hexdigest()
            row['poster_png_bytes'] = frame.stat().st_size
            after = source.stat()
            row['source_stat_unchanged'] = (before.st_size, before.st_mtime_ns) == (after.st_size, after.st_mtime_ns)
            assert row['source_stat_unchanged']
            print(json.dumps(row, ensure_ascii=False), flush=True)
    except Exception as e:
        report['error'] = repr(e)
        raise
    finally:
        (OUT / 'report.json').write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding='utf8')


if __name__ == '__main__':
    main()
