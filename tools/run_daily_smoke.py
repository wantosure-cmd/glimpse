"""Generated-only cross-entry native checks and a real separate-process restart."""
from pathlib import Path
import argparse, ctypes, hashlib, json, struct, subprocess, time, tomllib, uuid, zlib

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def chunk(kind, data):
    return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data) & 0xffffffff)


def application_version(exe):
    version = ctypes.WinDLL('version', use_last_error=True)
    version.GetFileVersionInfoSizeW.argtypes = [ctypes.c_wchar_p, ctypes.c_void_p]
    version.GetFileVersionInfoW.argtypes = [ctypes.c_wchar_p, ctypes.c_uint32, ctypes.c_uint32, ctypes.c_void_p]
    version.VerQueryValueW.argtypes = [ctypes.c_void_p, ctypes.c_wchar_p, ctypes.POINTER(ctypes.c_void_p), ctypes.POINTER(ctypes.c_uint)]
    size = version.GetFileVersionInfoSizeW(str(exe), None)
    assert size, 'Application version resource is missing'
    data = ctypes.create_string_buffer(size)
    assert version.GetFileVersionInfoW(str(exe), 0, size, data)
    pointer = ctypes.c_void_p()
    length = ctypes.c_uint()
    assert version.VerQueryValueW(data, '\\', ctypes.byref(pointer), ctypes.byref(length))
    words = ctypes.cast(pointer, ctypes.POINTER(ctypes.c_uint32))
    assert length.value >= 52 and words[0] == 0xfeef04bd
    return '.'.join(str(n) for n in (words[2] >> 16, words[2] & 0xffff, words[3] >> 16))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exe', type=Path, required=True)
    parser.add_argument('--scan-exit-only', action='store_true')
    args = parser.parse_args()
    exe = args.exe.resolve(strict=True)
    source_version = tomllib.loads((ROOT / 'Cargo.toml').read_text(encoding='utf8'))['package']['version']
    assert application_version(exe) == source_version, 'Wrong application version; refusing to start an unsupported diagnostic'
    task = ROOT / 'target' / ('daily-owned-' + uuid.uuid4().hex)
    media = task / 'B'
    older = task / 'A'
    work = task / 'work'
    for path in (media, older, work):
        path.mkdir(parents=True)
    (task / '.daily-owned').write_text('generated daily fixtures only', encoding='utf8')
    png = (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 320, 240, 8, 2, 0, 0, 0)) +
           chunk(b'IDAT', zlib.compress((b'\0' + bytes((45, 105, 175)) * 320) * 240)) + chunk(b'IEND', b''))
    for i in range(512):
        (older / f'a-{i:04}.png').write_bytes(png)
    (media / '01.png').write_bytes(png)
    (media / '02.png').write_bytes(png + b'\0' * 100)
    ffmpeg = next((p for p in (ROOT / 'vendor/runtime/ffmpeg.exe', exe.parent / 'runtime/ffmpeg.exe') if p.is_file()), None)
    assert ffmpeg, 'Restore the pinned runtime first'
    subprocess.run([str(ffmpeg), '-v', 'error', '-nostdin', '-f', 'lavfi', '-i', 'testsrc2=size=426x240:rate=12',
                    '-t', '30', '-c:v', 'mpeg4', '-q:v', '5', '-pix_fmt', 'yuv420p', '-an', str(media / '03.mov')],
                   check=True, creationflags=subprocess.CREATE_NO_WINDOW)
    (media / '04.mov').write_bytes((media / '03.mov').read_bytes())

    def identities():
        return {str(p.relative_to(task)): digest(p) for root in (media, older) for p in root.rglob('*') if p.is_file()}

    before = identities()
    result = dict(checks_pass=False, exe_path=str(exe), exe_sha256=digest(exe), task=str(task), runs=[],
                  scope='Generated media, native controlled input, separate process restart. No real library writes or physical-input certification.')
    output = ROOT / 'artifacts' / (task.name + '-verification.json')

    def save():
        output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n', encoding='utf8')

    save()
    modes = [(False, False), (True, False), (False, True), (True, True)]
    if args.scan_exit_only:
        modes = modes[2:]
    for restart, scan_exit in modes:
        name = ('scan-' if scan_exit else '') + ('restart' if restart else 'entries')
        raw = task / (name + '.json')
        command = [str(exe), '--daily-smoke', str(raw), '--data-dir', str(task / 'data'), '--instance-scope', task.name]
        command += ['--daily-restart'] if restart else ['--open', str(media)]
        if scan_exit:
            command += ['--daily-scan-exit']
        with (work / (name + '-stdout.log')).open('xb') as stdout, (work / (name + '-stderr.log')).open('xb') as stderr:
            proc = subprocess.Popen(command, cwd=work, stdout=stdout, stderr=stderr)
            run = dict(name=name, pid=proc.pid, active=True, command=command, raw=str(raw))
            result['runs'].append(run)
            save()
            began = time.monotonic()
            while proc.poll() is None:
                if time.monotonic() - began > 110:
                    raise TimeoutError(f'Owned daily test still live PID {proc.pid}; inspect before restarting')
                time.sleep(.1)
            run.update(active=False, exit_code=proc.returncode, seconds=time.monotonic() - began)
            save()
        report = json.loads(raw.read_text(encoding='utf8'))
        run['report'] = report
        save()
        assert proc.returncode == 0 and report['checks_pass'], report
        assert before == identities(), 'Generated media changed during cancellation/restart tests'
        if scan_exit and not restart:
            saved = json.loads((task / 'data/settings.json').read_text(encoding='utf8'))
            run['saved_last_root'] = saved['last_root']
            save()
            assert saved['last_root'] and Path(saved['last_root']).samefile(older), 'Exiting during scan lost the last opened directory'
    if not args.scan_exit_only:
        entries = result['runs'][0]['report']['rows']
        assert sorted(row['case'] for row in entries if row['name'] == 'entry-queues-exact-targets') == list(range(8))
        assert sorted(row['case'] for row in entries if row['name'] == 'entry-undone-without-side-effects') == list(range(8))
        assert any(row['name'] == 'pending-targets-survive-root-change' for row in entries)
        assert any(row['name'] == 'filtered-detail-next-video' for row in entries)
        assert any(row['name'] == 'exit-during-unsubmitted-delete' and row['trash']['pending'] for row in entries)
    assert digest(exe) == result['exe_sha256']
    result.update(checks_pass=True, source_files_checked=len(before), all_generated_media_unchanged=True,
                  eight_entry_routes_pass=not args.scan_exit_only, separate_process_restart_pass=True, scan_exit_restart_pass=True)
    save()
    print(json.dumps({key: result[key] for key in ('checks_pass', 'exe_sha256', 'task', 'source_files_checked',
          'eight_entry_routes_pass', 'separate_process_restart_pass')}, ensure_ascii=False))
    print('Evidence: ' + str(output))


if __name__ == '__main__':
    main()
