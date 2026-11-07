"""Stage hash-pinned native test dependencies and generate two owned codec fixtures."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import uuid
import zipfile
from fetch_p0 import ROOT, digest, fetch, fetch_mpv_archive


def stage_member(archive, name, expected, destination):
    with zipfile.ZipFile(archive) as package:
        members = [member for member in package.infolist() if not member.is_dir() and Path(member.filename).name == name]
        if len(members) != 1:
            raise RuntimeError('Expected exactly one dependency file: ' + name)
        data = package.read(members[0])
        if hashlib.sha256(data).hexdigest() != expected.lower():
            raise RuntimeError('Extracted dependency differs from product pin: ' + name)
        if destination.exists():
            if digest(destination) != expected.lower():
                raise RuntimeError('Refuse to overwrite a different runtime: ' + str(destination))
        else:
            destination.parent.mkdir(parents=True, exist_ok=True)
            with destination.open('xb') as output:
                output.write(data)


def main():
    runtime = ROOT / 'vendor/runtime'
    runtime.mkdir(parents=True, exist_ok=True)
    pins = json.loads((ROOT / 'vendor/runtime-manifest.json').read_text(encoding='utf-8-sig'))
    mpv = next(row for row in pins if row['file'] == 'libmpv-2.dll')
    stage_member(fetch_mpv_archive(), mpv['file'], mpv['sha256'], runtime / mpv['file'])
    tool_pins = json.loads((ROOT / 'vendor/media-tools.json').read_text(encoding='utf-8-sig'))
    spec = json.loads((ROOT / 'vendor/ci-dependencies.json').read_text(encoding='utf8'))['ffmpeg_archive']
    archive = fetch(spec['source'], spec['filename'], spec['sha256'])
    for pin in tool_pins:
        name = pin['component'] + '.exe'
        stage_member(archive, name, pin['sha256'], runtime / name)
    task = ROOT / 'target' / ('ci-fixtures-owned-' + uuid.uuid4().hex)
    task.mkdir(parents=True)
    (task / '.owned-fixtures').write_text('Generated codec clips only', encoding='ascii')
    media = ROOT / 'test-media'
    media.mkdir(exist_ok=True)
    rows = []
    for name, options in [
        ('test-h264-aac.mp4', ['-c:v', 'libx264', '-preset', 'ultrafast', '-c:a', 'aac', '-movflags', '+faststart']),
        ('test-mpeg4-mp3.avi', ['-c:v', 'mpeg4', '-q:v', '6', '-c:a', 'libmp3lame']),
    ]:
        destination = media / name
        created = not destination.exists()
        if created:
            generated = task / name
            subprocess.run([str(runtime / 'ffmpeg.exe'), '-v', 'error', '-nostdin', '-f', 'lavfi', '-i',
                            'testsrc2=size=160x96:rate=12', '-f', 'lavfi', '-i', 'sine=frequency=440:sample_rate=44100',
                            '-t', '2', '-pix_fmt', 'yuv420p', *options, str(generated)], check=True)
            with generated.open('rb') as source, destination.open('xb') as output:
                shutil.copyfileobj(source, output)
        probe = json.loads(subprocess.check_output([str(runtime / 'ffprobe.exe'), '-v', 'error', '-show_entries',
                           'stream=codec_type,codec_name', '-of', 'json', str(destination)]))
        codecs = {stream['codec_type']: stream['codec_name'] for stream in probe['streams']}
        expected = {'video': 'h264', 'audio': 'aac'} if name.endswith('.mp4') else {'video': 'mpeg4', 'audio': 'mp3'}
        if codecs != expected:
            raise RuntimeError('Unexpected codecs in dedicated test fixture: ' + name)
        rows.append(dict(file=name, created=created, codecs=codecs, sha256=digest(destination)))
    report = dict(checks_pass=True, libmpv_sha256=digest(runtime / mpv['file']), fixtures=rows,
                  scope='Null-output/player-release tests only, not GPU/physical audio/full format acceptance. No existing runtime or media overwritten.')
    (ROOT / 'artifacts').mkdir(exist_ok=True)
    (ROOT / 'artifacts/ci-runtime-preparation.json').write_text(json.dumps(report, indent=2), encoding='utf8')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
