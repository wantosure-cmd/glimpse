"""Observe only owned browser descendants using read-only Win32 process handles.

Never kill a process. Hold handles to distinguish identities and prevent PID reuse.
This observes normal automatic exits, not crashes, OS logoff or physical input.
"""
import argparse
import ctypes as C
from ctypes import wintypes as W
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import time
import uuid

ROOT = Path(__file__).resolve().parent.parent
K = C.WinDLL('kernel32', use_last_error=True)


class Entry(C.Structure):
    _fields_ = [('size', W.DWORD), ('usage', W.DWORD), ('pid', W.DWORD),
                ('heap', C.c_size_t), ('module', W.DWORD), ('threads', W.DWORD),
                ('ppid', W.DWORD), ('priority', W.LONG), ('flags', W.DWORD),
                ('exe', W.WCHAR * 260)]


for name, args, result in (
    ('CreateToolhelp32Snapshot', [W.DWORD, W.DWORD], W.HANDLE),
    ('Process32FirstW', [W.HANDLE, C.POINTER(Entry)], W.BOOL),
    ('Process32NextW', [W.HANDLE, C.POINTER(Entry)], W.BOOL),
    ('OpenProcess', [W.DWORD, W.BOOL, W.DWORD], W.HANDLE),
    ('CloseHandle', [W.HANDLE], W.BOOL),
    ('QueryFullProcessImageNameW', [W.HANDLE, W.DWORD, W.LPWSTR, C.POINTER(W.DWORD)], W.BOOL),
    ('GetProcessTimes', [W.HANDLE] + [C.POINTER(W.FILETIME)] * 4, W.BOOL),
    ('WaitForSingleObject', [W.HANDLE, W.DWORD], W.DWORD),
    ('GetExitCodeProcess', [W.HANDLE, C.POINTER(W.DWORD)], W.BOOL),
    ('GetSystemDirectoryW', [W.LPWSTR, W.UINT], W.UINT),
):
    f = getattr(K, name)
    f.argtypes, f.restype = args, result


def snapshot():
    handle = K.CreateToolhelp32Snapshot(2, 0)
    if handle == C.c_void_p(-1).value:
        raise C.WinError(C.get_last_error())
    rows = []
    try:
        entry = Entry()
        entry.size = C.sizeof(entry)
        if not K.Process32FirstW(handle, C.byref(entry)):
            raise C.WinError(C.get_last_error())
        while True:
            rows.append((entry.pid, entry.ppid, entry.exe))
            if not K.Process32NextW(handle, C.byref(entry)):
                if C.get_last_error() != 18:
                    raise C.WinError(C.get_last_error())
                return rows
    finally:
        K.CloseHandle(handle)


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def identity(pid):
    handle = K.OpenProcess(0x1000 | 0x100000, False, pid)
    if not handle:
        raise C.WinError(C.get_last_error())
    try:
        size = W.DWORD(32768)
        image = C.create_unicode_buffer(size.value)
        if not K.QueryFullProcessImageNameW(handle, 0, image, C.byref(size)):
            raise C.WinError(C.get_last_error())
        times = [W.FILETIME() for _ in range(4)]
        if not K.GetProcessTimes(handle, *(C.byref(t) for t in times)):
            raise C.WinError(C.get_last_error())
        stamp = times[0].dwLowDateTime | (times[0].dwHighDateTime << 32)
        return handle, {'pid': pid, 'creation_filetime': stamp, 'image': image.value}
    except BaseException:
        K.CloseHandle(handle)
        raise


def terminated(handle):
    result = K.WaitForSingleObject(handle, 0)
    if result not in (0, 258):
        raise C.WinError(C.get_last_error())
    return result == 0


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--delivery', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--quit-ms', type=int, nargs=3, default=[400, 800, 1200])
    a = p.parse_args()
    assert all(100 <= value <= 10000 for value in a.quit_ms) and len(set(a.quit_ms)) == 3
    delivery = a.delivery.resolve(strict=True)
    output = a.output.resolve()
    assert delivery.is_relative_to(ROOT / 'dist') and output.is_relative_to(ROOT / 'artifacts')
    assert not output.exists()
    manifest = json.loads((delivery / 'delivery.json').read_text(encoding='utf8'))
    system_path = C.create_unicode_buffer(32768)
    length = K.GetSystemDirectoryW(system_path, len(system_path))
    assert 0 < length < len(system_path)
    console_host = (Path(system_path.value) / 'conhost.exe').resolve(strict=True)
    exe = delivery / 'media-browser-portable/media-browser.exe'
    assert sha(exe) == manifest['exe_sha256']
    package_files = json.loads((exe.parent / 'package-files.json').read_text(encoding='utf8'))
    runtime_pins = {r['path'].removeprefix('runtime/'): r['sha256'] for r in package_files
                    if r['path'].startswith('runtime/')}
    task = ROOT / 'target' / ('decoder-exit-owned-' + uuid.uuid4().hex)
    task.mkdir()
    (task / '.decoder-exit-owned').write_text('Generated copies, read-only process observations.', encoding='ascii')
    source = ROOT / 'test-media/test-mpeg4-mp3.avi'
    original_hash = sha(source)
    for folder, count in (('detail', 1), ('busy', 8)):
        media = task / folder
        media.mkdir()
        for number in range(count):
            shutil.copyfile(source, media / f'{number:02d}.avi')
    before = {str(f.relative_to(task)): sha(f) for f in task.glob('*/*.avi')}
    result = {'finished': False, 'checks_pass': False, 'full_acceptance_pass': False,
              'exe_sha256': manifest['exe_sha256'], 'task': str(task), 'rows': [],
              'requested_quit_ms': a.quit_ms,
              'poll_interval_ms': 20, 'scope': 'Read-only Toolhelp snapshots and retained process handles for owned browser descendants. First observed app exit and a two-second follow-up. Normal format diagnostic completion and early automatic quit only; no forced process termination, crash, OS shutdown, physical input or exhaustive creation-event tracing.'}

    def save():
        output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf8')

    try:
        for name, delay in [('detail-normal', None)] + [(f'busy-{ms}', ms) for ms in a.quit_ms]:
            work = task / name
            work.mkdir()
            media = task / ('detail' if delay is None else 'busy')
            args = [str(exe), '--open', str(media), '--data-dir', str(work / 'data'),
                    '--instance-scope', task.name + name]
            args += ['--format-smoke', str(work / 'native')] if delay is None else ['--quit-after', str(delay)]
            row = {'name': name, 'finished': False, 'children': [], 'exited_before_handle_query': []}
            result['rows'].append(row)
            handles, records = {}, {}
            with (work / 'stdout.log').open('wb') as out, (work / 'stderr.log').open('wb') as err:
                app = subprocess.Popen(args, cwd=work, stdout=out, stderr=err)
                row['app_pid'] = app.pid
                result['active_pid'] = app.pid
                began = time.monotonic()
                try:
                    handle, root = identity(app.pid)
                    handles[app.pid] = handle
                    row['app_identity'] = root
                    save()

                    def observe():
                        entries = snapshot()
                        pending = True
                        while pending:
                            pending = False
                            for pid, ppid, image in entries:
                                if ppid not in handles or pid in handles:
                                    continue
                                try:
                                    handle, entry = identity(pid)
                                except OSError as exc:
                                    if exc.winerror == 87 and pid not in {r[0] for r in snapshot()}:
                                        if pid not in row['exited_before_handle_query']:
                                            row['exited_before_handle_query'].append(pid)
                                        continue
                                    raise
                                parent = root if ppid == app.pid else records[ppid]
                                if entry['creation_filetime'] < parent['creation_filetime']:
                                    # Toolhelp can contain old orphans with a stale reused parent PID.
                                    K.CloseHandle(handle)
                                    continue
                                handles[pid] = handle
                                entry.update(parent_pid=ppid, snapshot_name=image,
                                             first_observed_ms=(time.monotonic() - began) * 1000)
                                records[pid] = entry
                                row['children'].append(entry)
                                pending = True

                    while app.poll() is None:
                        observe()
                        if time.monotonic() - began > 60:
                            row['state'] = 'still-live; inspect this PID, do not restart or kill'
                            raise TimeoutError(row['state'])
                        time.sleep(.02)
                    row['app_exit_code'] = app.returncode
                    row['first_observed_app_exit_ms'] = (time.monotonic() - began) * 1000
                    observe()
                    row['alive_at_first_observed_app_exit'] = [pid for pid in records if not terminated(handles[pid])]
                    until = time.monotonic() + 2
                    while time.monotonic() < until:
                        observe()
                        time.sleep(.02)
                    row['alive_after_followup'] = [pid for pid in records if not terminated(handles[pid])]
                    for pid, entry in records.items():
                        entry['terminated'] = terminated(handles[pid])
                        code = W.DWORD()
                        assert K.GetExitCodeProcess(handles[pid], C.byref(code))
                        entry['exit_code'] = code.value
                        image = Path(entry['image']).resolve(strict=True)
                        if image.parent == exe.parent / 'runtime':
                            entry['kind'] = 'bundled-runtime'
                        else:
                            assert image == console_host, entry
                            entry['kind'] = 'system-console-host'
                        entry['image_sha256'] = sha(image)
                        if entry['kind'] == 'bundled-runtime':
                            assert entry['image_sha256'] == runtime_pins[image.name], entry
                    assert app.returncode == 0 and records, row
                    assert not row['alive_at_first_observed_app_exit'] and not row['alive_after_followup'], row
                    if delay is None:
                        native = json.loads((work / 'native/report.json').read_text(encoding='utf8'))
                        assert native['error'] is None
                    row['finished'] = True
                    result['active_pid'] = None
                    save()
                    print(name, 'PASS', len(records), 'observed children', flush=True)
                finally:
                    for handle in handles.values():
                        K.CloseHandle(handle)
        assert {Path(r['image']).name.lower() for row in result['rows'] for r in row['children']} >= {'ffmpeg.exe', 'ffprobe.exe'}
        assert before == {str(f.relative_to(task)): sha(f) for f in task.glob('*/*.avi')}
        assert sha(source) == original_hash and sha(exe) == manifest['exe_sha256']
        result.update(finished=True, checks_pass=True, sources_unchanged=True)
    finally:
        save()


if __name__ == '__main__':
    main()
