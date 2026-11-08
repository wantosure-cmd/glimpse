"""Measure native next/previous video rendering on identical local and optional read-only mounted sources."""
from pathlib import Path
import argparse
import heapq
import json
import math
import os
import shutil
import struct
import subprocess
import time
import tomllib
import uuid
from run_daily_smoke import application_version, digest

ROOT = Path(__file__).resolve().parents[1]


def identity(path):
    before = path.stat()
    value = digest(path)
    after = path.stat()
    assert (before.st_size, before.st_mtime_ns) == (after.st_size, after.st_mtime_ns), 'Source changed while hashing'
    return dict(bytes=after.st_size, modified_ns=after.st_mtime_ns, sha256=value)


def stage_application(exe, task, omit_mpv=False):
    # Cargo can use a target outside this checkout. native::runtime_dir deliberately
    # refuses to load loose DLLs there: stage an explicit adjacent trusted runtime.
    application = task / 'application'
    runtime = application / 'runtime'
    runtime.mkdir(parents=True)
    runtime_source = exe.parent / 'runtime' if (exe.parent / 'runtime').is_dir() else ROOT / 'vendor/runtime'
    pins = json.loads((ROOT / 'vendor/runtime-manifest.json').read_text(encoding='utf-8-sig'))
    pins += [dict(file=p['component']+'.exe', sha256=p['sha256'])
             for p in json.loads((ROOT / 'vendor/media-tools.json').read_text(encoding='utf-8-sig'))]
    for pin in pins:
        source = runtime_source / pin['file']
        assert digest(source) == pin['sha256'].lower(), 'Native runtime differs from locked identity: ' + pin['file']
        if omit_mpv and pin['file'] == 'libmpv-2.dll':
            continue  # intentional omission in this new task-owned application only
        destination = runtime / pin['file']
        try:
            os.link(source, destination)  # read-only runtime; bounded disk use on same volume
        except OSError:
            shutil.copyfile(source, destination)
    staged_exe = application / 'media-browser.exe'
    shutil.copyfile(exe, staged_exe)
    assert digest(staged_exe) == digest(exe)
    shutil.copyfile(runtime / 'vcruntime140.dll', application / 'vcruntime140.dll')
    return staged_exe, runtime, runtime_source, pins


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exe', type=Path, required=True)
    parser.add_argument('--directory', type=Path)
    parser.add_argument('--selection', choices=('smallest', 'spread'), default='smallest')
    parser.add_argument('--init-recovery', action='store_true', help='Generated-only missing DLL, failed retry, restore and real retry test')
    parser.add_argument('--pairs', type=int, default=2, choices=(1, 2))
    parser.add_argument('--transitions', type=int, default=20, choices=range(20, 41))
    args = parser.parse_args()
    assert not args.init_recovery or (args.directory is None and args.pairs == 1), 'Init recovery accepts one generated pair only'
    exe = args.exe.resolve(strict=True)
    assert application_version(exe) == tomllib.loads((ROOT / 'Cargo.toml').read_text(encoding='utf8'))['package']['version']
    task = ROOT / 'target' / ('switch-owned-' + uuid.uuid4().hex)
    task.mkdir(parents=True)
    (task / '.switch-owned').write_text('read-only source switch test', encoding='ascii')
    staged_exe, runtime, runtime_source, pins = stage_application(exe, task, args.init_recovery)
    local = task / 'local-media'
    local.mkdir()
    generated = task / 'generated-media'
    generated.mkdir()
    if args.directory:
        # CloudDrive can enumerate/read normally while GetFinalPathNameByHandle
        # (used by Path.resolve) returns ERROR_UNRECOGNIZED_VOLUME (1005).
        source_root = Path(os.path.abspath(args.directory))
        assert source_root.is_dir(), 'Mounted source is not an accessible directory'
        # Direct children only; never traverse/rewrite the real library or follow file symlinks.
        candidates = ((p.stat().st_size, p.name.lower(), p) for p in source_root.iterdir()
                      if not p.is_symlink() and p.is_file() and p.suffix.lower() == '.mp4')
        if args.selection == 'smallest':
            selected = [row[2] for row in heapq.nsmallest(3, candidates)]
        else:
            # Broader bytes/length sample, still bounded: min, middle, max of
            # <=32MiB direct MP4s, with <=64MiB copied in total.
            eligible = sorted(row for row in candidates if 0 < row[0] <= 32 * 1024**2)
            assert len(eligible) >= 3, 'Need three distinct sources inside bounded copy budget'
            first, last = eligible[0], eligible[-1]
            middle = [row for row in eligible[1:-1] if row[0] + first[0] + last[0] <= 64 * 1024**2]
            assert middle, 'No middle source within total read budget'
            selected = [first[2], middle[len(middle)//2][2], last[2]]
        assert len(selected) == 3 and max(p.stat().st_size for p in selected) <= 32 * 1024**2
        assert sum(p.stat().st_size for p in selected) <= 64 * 1024**2, 'Bounded read-only copy budget exceeded'
    else:
        source_root = generated
        ffmpeg = next((p for p in (ROOT / 'vendor/runtime/ffmpeg.exe', exe.parent / 'runtime/ffmpeg.exe') if p.is_file()), None)
        assert ffmpeg, 'Restore pinned runtime first'
        for index, size in enumerate(('426x240', '240x426', '640x360')):
            subprocess.run([str(ffmpeg), '-v', 'error', '-nostdin', '-f', 'lavfi', '-i', f'testsrc2=size={size}:rate=24',
                            '-t', '30', '-c:v', 'mpeg4', '-q:v', '6', '-an', str(generated / f'{index+1:02}.mp4')],
                           check=True, creationflags=subprocess.CREATE_NO_WINDOW)
        selected = sorted(generated.glob('*.mp4'))
    before = {p.name: identity(p) for p in selected}
    for source in selected:
        with source.open('rb') as incoming, (local / source.name).open('xb') as output:
            shutil.copyfileobj(incoming, output, length=1024*1024)
        assert digest(local / source.name) == before[source.name]['sha256']
    result = dict(checks_pass=False, exe_path=str(exe), staged_exe=str(staged_exe), exe_sha256=digest(exe), task=str(task),
                  runtime_files_verified=len(pins),
                  selection=args.selection,
                  sources=before, source_root=str(source_root), runs=[],
                  scope='Same three unchanged files; hashing/copy prewarms source/driver caches. Fresh app data each run; source/local order counterbalanced. Controlled native input -> FBO ready render, not physical presentation/audio or network cold-cache proof.')
    output = ROOT / 'artifacts' / (task.name + '-verification.json')
    output.parent.mkdir(exist_ok=True)

    def save():
        output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n', encoding='utf8')

    save()
    for pair in range(args.pairs):
        for mode in (('source', 'local') if pair % 2 == 0 else ('local', 'source')):
            name = f'{pair+1}-{mode}'
            root = source_root if mode == 'source' else local
            manifest = task / (name + '-sources.json')
            init_recovery = args.init_recovery and pair == 0 and mode == 'source'
            manifest.write_text(json.dumps(dict(paths=[str(root / p.name) for p in selected], transitions=args.transitions,
                                                init_recovery=init_recovery)), encoding='utf8')
            raw = task / (name + '-native.json')
            command = [str(staged_exe), '--open', str(root), '--data-dir', str(task / (name + '-data')),
                       '--instance-scope', task.name + '-' + name, '--video-switch-smoke', str(raw), '--switch-sources', str(manifest)]
            with (task / (name + '-stdout.log')).open('xb') as stdout, (task / (name + '-stderr.log')).open('xb') as stderr:
                proc = subprocess.Popen(command, cwd=task, stdout=stdout, stderr=stderr)
                row = dict(pair=pair+1, mode=mode, pid=proc.pid, active=True, command=command, raw=str(raw))
                result['runs'].append(row)
                save()
                began = time.monotonic()
                restored = False
                while proc.poll() is None:
                    failure = raw.with_suffix('.init-failure.json')
                    if init_recovery and not restored and failure.exists():
                        try:
                            observation = json.loads(failure.read_text(encoding='utf8'))
                        except (json.JSONDecodeError, PermissionError):
                            # The native sidecar is a live handshake; allow its
                            # short write to finish, while the native timeout stays active.
                            time.sleep(.02)
                            continue
                        assert observation['negative_retry_confirmed'] and observation['error']
                        destination = runtime / 'libmpv-2.dll'
                        assert not destination.exists(), 'Expected omitted DLL only in owned runtime'
                        source = runtime_source / destination.name
                        try:
                            os.link(source, destination)
                        except OSError:
                            with source.open('rb') as incoming, destination.open('xb') as output_stream:
                                shutil.copyfileobj(incoming, output_stream)
                        pin = next(p for p in pins if p['file'] == destination.name)
                        assert digest(destination) == pin['sha256'].lower()
                        raw.with_suffix('.runtime-restored').write_bytes(b'restored verified runtime')
                        row['init_failure_observation'] = observation
                        restored = True
                        save()
                    if time.monotonic() - began > 115 + args.transitions * 21:
                        raise TimeoutError(f'Owned switch process still live PID {proc.pid}; inspect before restarting')
                    time.sleep(.1)
                row.update(active=False, exit_code=proc.returncode, process_seconds=time.monotonic()-began)
                save()
            report = json.loads(raw.read_text(encoding='utf8'))
            row['report'] = report
            save()
            assert proc.returncode == 0 and report['checks_pass'] and len(report['rows']) == args.transitions, report
            if init_recovery:
                assert restored and report['initialization_recovered'] and report['negative_retry_confirmed']
                # Verify the actual native FBO contains the visible red error,
                # not merely a retained error field or an allocated retry rect.
                picture = raw.with_suffix('.init-failure.png')
                width, height = struct.unpack('>II', picture.read_bytes()[16:24])
                assert 0 < width <= 8192 and 0 < height <= 8192
                rgb = subprocess.check_output([str(runtime / 'ffmpeg.exe'), '-v', 'error', '-nostdin', '-i', str(picture),
                                               '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'],
                                              creationflags=subprocess.CREATE_NO_WINDOW)
                assert len(rgb) == width*height*3
                x, y, w, h = row['init_failure_observation']['canvas']
                scale = row['init_failure_observation']['pixels_per_point']
                red = 0
                for py in range(max(0, int(y*scale)), min(height, int((y+h)*scale))):
                    for px in range(max(0, int(x*scale)), min(width, int((x+w)*scale))):
                        at = (py*width+px)*3
                        r, g, b = rgb[at:at+3]
                        red += r > 140 and r > g*1.25 and r > b*1.25
                assert red >= 20, 'Missing visible init-error pixels in native FBO'
                row['init_error_red_pixels'] = red
            values = sorted(item['input_to_ready_render_ms'] for item in report['rows'])
            assert all(math.isfinite(v) and v > 0 for v in values)
            row['timing_ms'] = dict(median=(values[(len(values)-1)//2]+values[len(values)//2])/2,
                                    p95=values[math.ceil(.95*len(values))-1], maximum=max(values), count=len(values))
            assert before == {p.name: identity(p) for p in selected}, 'Read-only source identity changed'
            assert all(digest(local / p.name) == before[p.name]['sha256'] for p in selected)
            save()
    sequence = lambda run: [(Path(v['from']).name, Path(v['to']).name, v['direction']) for v in run['report']['rows']]
    reference = result['runs'][0]
    for row in result['runs']:
        assert sequence(row) == sequence(reference), 'Local/source navigation sequences differ'
        assert row['report']['viewport'] == reference['report']['viewport']
        assert row['report']['pixels_per_point'] == reference['report']['pixels_per_point']
    assert digest(exe) == result['exe_sha256']
    assert digest(staged_exe) == result['exe_sha256']
    assert all(digest(runtime / p['file']) == p['sha256'].lower() for p in pins)
    result.update(checks_pass=True, all_sources_and_copies_unchanged=True, matching_sequence_and_geometry=True,
                  initialization_recovery_pass=args.init_recovery)
    save()
    print(json.dumps(dict(checks_pass=True, evidence=str(output), runs=[dict(mode=r['mode'], pair=r['pair'], **r['timing_ms']) for r in result['runs']]), ensure_ascii=False))


if __name__ == '__main__':
    main()
