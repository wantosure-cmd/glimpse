"""Generate a known H.264 fixture and verify actual seek readback observations."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import uuid

ROOT = Path(__file__).resolve().parent.parent


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--exe', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--pilot', action='store_true', help='Correctness exploration, no performance acceptance claim')
    a = p.parse_args()
    exe = a.exe.resolve(strict=True)
    output = a.output.resolve()
    assert output.is_relative_to(ROOT / 'artifacts') and not output.exists()
    output.parent.mkdir(parents=True, exist_ok=True)
    assert exe.is_relative_to(ROOT / 'target') or exe.is_relative_to(ROOT / 'dist')
    if not a.pilot:
        assert exe.is_relative_to(ROOT / 'dist'), 'Performance series must use a delivered Release binary'
    runtime = exe.parent / 'runtime'
    if not runtime.is_dir():
        assert exe.parent in (ROOT / 'target/debug', ROOT / 'target/release')
        runtime = ROOT / 'vendor/runtime'
    ffmpeg, ffprobe = runtime / 'ffmpeg.exe', runtime / 'ffprobe.exe'
    for tool in json.loads((ROOT / 'vendor/media-tools.json').read_text(encoding='utf-8-sig')):
        assert sha(runtime / (tool['component'] + '.exe')).lower() == tool['sha256'].lower()
    task = ROOT / 'target' / ('seek-readback-owned-' + uuid.uuid4().hex)
    media, work = task / 'media', task / 'work'
    media.mkdir(parents=True)
    work.mkdir()
    (media / '.seek-benchmark-owned').write_text('Generated color video only.', encoding='ascii')
    palette = [(64 + (i % 3) * 64, 64 + ((i // 3) % 3) * 64, 64 + (i // 9) * 64) for i in range(20)]
    order = [(i * 7 + 7) % 20 for i in range(20)]
    reference = [{'target': index * 3 + 1.5, 'rgb': palette[index]} for index in order]
    (media / 'seek-reference.json').write_text(json.dumps(reference), encoding='utf8')
    filters = []
    for index, rgb in enumerate(palette):
        color = ''.join(f'{v:02x}' for v in rgb)
        filters.append(f"drawbox=x=iw/2-32:y=ih/2-32:w=64:h=64:color=0x{color}:t=fill:enable='gte(t,{index*3})*lt(t,{(index+1)*3})'")
    filters.append('drawbox=x=850:y=600:w=220:h=24:color=black:t=fill')
    for bit in range(11):
        filters.append(f"drawbox=x={850+20*bit}:y=600:w=14:h=24:color=white:t=fill:enable='mod(floor((t*30+0.5)/{1<<bit}),2)'")
    # Keep moving complex content; the small central color marker identifies a target segment.
    # drawbox's RGB-to-YUV colors are tagged explicitly instead of leaving an HD matrix guess.
    cmd = [str(ffmpeg), '-v', 'error', '-f', 'lavfi', '-i', 'testsrc2=s=1920x1080:r=30:d=60',
           '-vf', ','.join(filters), '-c:v', 'libx264', '-preset', 'ultrafast', '-crf', '18',
           '-threads', '4', '-g', '60', '-pix_fmt', 'yuv420p', '-colorspace', 'smpte170m',
           '-color_primaries', 'bt709', '-color_trc', 'bt709', '-color_range', 'tv', str(media / 'sample.mp4')]
    with (task / 'fixture.log').open('wb') as log:
        subprocess.run(cmd, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=180,
                       creationflags=subprocess.CREATE_NO_WINDOW)
    probe = json.loads(subprocess.check_output([str(ffprobe), '-v', 'error', '-show_streams', '-show_format',
                        '-of', 'json', str(media / 'sample.mp4')], creationflags=subprocess.CREATE_NO_WINDOW))
    video = next(s for s in probe['streams'] if s['codec_type'] == 'video')
    assert video['codec_name'] == 'h264' and (video['width'], video['height']) == (1920, 1080)
    assert video['r_frame_rate'] == '30/1' and abs(float(probe['format']['duration']) - 60) < .1
    assert video['color_space'] == 'smpte170m'
    decoded = subprocess.check_output([str(ffmpeg), '-v', 'error', '-i', str(media / 'sample.mp4'),
                  '-vf', "select='eq(mod(n,90),45)',crop=8:8:956:536,format=rgb24", '-fps_mode', 'passthrough',
                  '-f', 'rawvideo', '-'], creationflags=subprocess.CREATE_NO_WINDOW, timeout=90)
    assert len(decoded) == 20 * 8 * 8 * 3
    source_colors = [[sum(decoded[i*192+k:(i+1)*192:3])//64 for k in range(3)] for i in range(20)]
    assert all(max(abs(a-b) for a,b in zip(rgb, expected)) <= 6 for rgb, expected in zip(source_colors, palette))
    codes = subprocess.check_output([str(ffmpeg), '-v', 'error', '-i', str(media / 'sample.mp4'),
                  '-vf', "select='eq(mod(n,90),45)',crop=220:24:850:600,format=rgb24", '-fps_mode', 'passthrough',
                  '-f', 'rawvideo', '-'], creationflags=subprocess.CREATE_NO_WINDOW, timeout=90)
    assert len(codes) == 20 * 220 * 24 * 3
    for i in range(20):
        number = 0
        for bit in range(11):
            offset = (i * 220 * 24 + 12 * 220 + 7 + 20 * bit) * 3
            level = sum(codes[offset:offset+3]) // 3
            assert level < 48 or level > 208
            number |= int(level > 208) << bit
        assert number == 45 + 90 * i, (i, number)
    before = {f.name: sha(f) for f in media.iterdir() if f.is_file()}
    raw = work / 'seek.json'
    result = {'checks_pass': False, 'full_acceptance_pass': False, 'pilot': a.pilot,
              'exe_sha256': sha(exe), 'task': str(task), 'raw': str(raw), 'fixture': probe,
              'sources_before': before, 'physical_input_verified': False, 'physical_presentation_verified': False,
              'decoded_source_marker_rgb': source_colors,
              'scope': 'Synthetic slider input to native GPU readback with segment color and 11-bit frame counter within two frames of target. Generated moving 1080p H.264/30fps, 20 nonadjacent targets. Saved PNG frame codes independently decoded. Not physical presentation/input or general real-world seek P95; no failed sample discarded.'}
    try:
        with (work / 'stdout.log').open('wb') as out, (work / 'stderr.log').open('wb') as err:
            app = subprocess.Popen([str(exe), '--open', str(media), '--data-dir', str(task / 'data'),
                                    '--seek-benchmark', str(raw), '--instance-scope', task.name],
                                   cwd=work, stdout=out, stderr=err)
            result['pid'] = app.pid
            output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf8')
            try:
                result['exit_code'] = app.wait(timeout=110)
            except subprocess.TimeoutExpired:
                result['state'] = 'still-live; inspect this PID before any restart; no process killed'
                raise
        observed = json.loads(raw.read_text(encoding='utf8'))
        result['observed'] = observed
        assert result['exit_code'] == 0 and observed['checks_pass'] and len(observed['rows']) == 20, observed
        for expected, row in zip(reference, observed['rows']):
            assert row['reference_target'] == expected['target'] and row['expected_rgb'] == list(expected['rgb'])
            assert max(abs(a-b) for a,b in zip(row['expected_rgb'], row['observed_rgb'])) <= 18
            assert abs(row['observed_time']-row['slider_target']) <= .2 and row['screenshot_saved']
            assert abs(row['observed_frame_time']-row['slider_target']) <= 2/30 + .0001
            assert Path(row['screenshot']).is_file() and row['elapsed_ms'] > 0
            image = Path(row['screenshot']).resolve(strict=True)
            assert image.is_relative_to(work)
            rgb = subprocess.check_output([str(ffmpeg), '-v', 'error', '-i', str(image),
                        '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'], creationflags=subprocess.CREATE_NO_WINDOW)
            width, height = row['pixels']
            assert len(rgb) == width * height * 3
            left, top, right, bottom = row['video_region_pixels']
            number = 0
            for bit in range(11):
                x = int(left + (right-left) * (857+20*bit) / 1920 + .5)
                y = int(top + (bottom-top) * 612 / 1080 + .5)
                values = [rgb[(yy*width+xx)*3+k] for yy in range(y-1,y+2) for xx in range(x-1,x+2) for k in range(3)]
                level = sum(values)//27
                assert level < 48 or level > 208
                number |= int(level > 208) << bit
            assert number == row['observed_frame_number'] and abs(number/30-row['slider_target']) <= 2/30+.0001
            row['independent_png_frame_code_verified'] = True
        after = {f.name: sha(f) for f in media.iterdir() if f.is_file()}
        assert after == before and sha(exe) == result['exe_sha256']
        result.update(checks_pass=True, sources_unchanged=True,
                      native_readback_p95_ms=observed['p95_ms'], raw_sha256=sha(raw))
        print(json.dumps({k: result[k] for k in ('checks_pass', 'pilot', 'exe_sha256', 'native_readback_p95_ms', 'scope')}, indent=2))
    finally:
        output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf8')


if __name__ == '__main__':
    main()
