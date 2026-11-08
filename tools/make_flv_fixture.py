"""Add two owned real FLV fixtures without replacing an existing format matrix."""
import hashlib
import json
import subprocess
import uuid
from make_format_fixture import ROOT, BASE, FP, execute


def main():
    assert (BASE / '.generated-format-fixture').is_file(), 'Prepare the generated format fixture root first'
    task = BASE / ('flv-owned-' + uuid.uuid4().hex)
    task.mkdir()
    (task / '.flv-owned').write_text('generated FLV samples only', encoding='ascii')
    cases = []
    for name, extension, video, audio, options in [
        ('h264-aac-flv', 'flv', 'h264', 'aac', ['-c:v', 'libx264', '-preset', 'ultrafast', '-c:a', 'aac']),
        ('flv1-mp3-flv', 'FLV', 'flv1', 'mp3', ['-c:v', 'flv', '-q:v', '5', '-c:a', 'libmp3lame']),
    ]:
        folder = task / name
        folder.mkdir()
        path = folder / ('sample.' + extension)
        execute(['-f', 'lavfi', '-i', 'testsrc2=size=640x360:rate=15:duration=65',
                 '-f', 'lavfi', '-i', 'sine=frequency=440:sample_rate=48000:duration=65',
                 *options, '-pix_fmt', 'yuv420p', '-threads', '2', '-shortest', '-f', 'flv', path])
        probe = json.loads(subprocess.check_output([str(FP), '-v', 'error', '-show_streams', '-show_format', '-of', 'json', str(path)],
                                                  creationflags=subprocess.CREATE_NO_WINDOW))
        assert probe['format']['format_name'] == 'flv'
        assert {s['codec_type']: s['codec_name'] for s in probe['streams']} == dict(video=video, audio=audio)
        cases.append(dict(name=name, kind='video', path=str(path.relative_to(ROOT)),
                          expected=dict(video_codec=video, audio_codec=audio, width=640, height=360, duration_min=64),
                          provenance='Generated testsrc2 and sine, genuine FLV muxer; no renamed containers or private media',
                          bytes=path.stat().st_size, sha256=hashlib.file_digest(path.open('rb'), 'sha256').hexdigest(), ffprobe=probe))
    manifest = task / 'manifest.json'
    manifest.write_text(json.dumps(dict(scope='Two genuine generated FLV codec combinations, lower/uppercase extensions', cases=cases),
                                   ensure_ascii=False, indent=2), encoding='utf8')
    report = dict(root=str(task), manifest=str(manifest), cases=len(cases))
    (ROOT / 'artifacts/flv56-fixture.json').write_text(json.dumps(report, indent=2), encoding='utf8')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
