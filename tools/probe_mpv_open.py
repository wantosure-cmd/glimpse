"""Pinned libmpv diagnostic: separate demux/open delay from native GPU/UI timing."""
from pathlib import Path
import argparse
import ctypes as C
import hashlib
import json
import os
import time

ROOT = Path(__file__).resolve().parent.parent


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--runtime', type=Path, required=True)
    parser.add_argument('--profile', choices=['baseline', 'analyze1s', 'lazyindex', 'smallbuffers', 'winrandom', 'winnormal', 'winrandomsmall', 'winunbuffered', 'winrandomsmallnoprobe'], required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--source-check', choices=['full', 'sampled'], default='full')
    parser.add_argument('--seek-targets', default='5,35,780,1500', help='Comma-separated seconds; seek completion always requires playback restart')
    args = parser.parse_args()
    source, runtime = args.source.resolve(strict=True), args.runtime.resolve(strict=True)
    output = args.output.resolve()
    if output.exists() or not output.is_relative_to(ROOT / 'artifacts'):
        raise ValueError('Use a new artifact report')
    dll = runtime / 'libmpv-2.dll'
    pin = next(row['sha256'].lower() for row in json.loads((ROOT / 'vendor/runtime-manifest.json').read_text())
               if row['file'] == dll.name)
    assert sha(dll) == pin, 'Unexpected libmpv build'
    before_stat = source.stat()
    def source_identity():
        if args.source_check == 'full':
            return dict(full_sha256=sha(source))
        size = source.stat().st_size
        offsets = sorted({0, min(1048576, max(0, size - 4096)), size // 2, max(0, size - 4096)})
        rows = []
        with source.open('rb') as stream:
            for offset in offsets:
                stream.seek(offset)
                data = stream.read(min(4096, size - offset))
                rows.append(dict(offset=offset, bytes=len(data), sha256=hashlib.sha256(data).hexdigest()))
        return dict(sampled_ranges=rows)

    before = source_identity()
    directory = os.add_dll_directory(str(runtime))
    lib = C.CDLL(str(dll))

    class Event(C.Structure):
        _fields_ = [('id', C.c_int), ('error', C.c_int), ('cookie', C.c_uint64), ('data', C.c_void_p)]

    def bind(name, result, *parameters):
        function = getattr(lib, name)
        function.restype, function.argtypes = result, list(parameters)
        return function

    create = bind('mpv_create', C.c_void_p)
    option = bind('mpv_set_option_string', C.c_int, C.c_void_p, C.c_char_p, C.c_char_p)
    initialize = bind('mpv_initialize', C.c_int, C.c_void_p)
    get = bind('mpv_get_property_string', C.c_void_p, C.c_void_p, C.c_char_p)
    release = bind('mpv_free', None, C.c_void_p)
    command = bind('mpv_command_async', C.c_int, C.c_void_p, C.c_uint64, C.POINTER(C.c_char_p))
    wait = bind('mpv_wait_event', C.POINTER(Event), C.c_void_p, C.c_double)
    destroy = bind('mpv_terminate_destroy', None, C.c_void_p)
    handle = create()
    assert handle

    def text(name):
        pointer = get(handle, name.encode())
        if not pointer:
            return None
        try:
            return C.string_at(pointer).decode('utf8', errors='replace')
        finally:
            release(pointer)

    def send(*words):
        values = [word.encode('utf8') for word in words] + [None]
        argv = (C.c_char_p * len(values))(*values)
        assert command(handle, 1, argv) >= 0

    options = dict(config='no', terminal='no', vo='null', ao='null', idle='yes',
                   **{'input-default-bindings': 'no', 'input-vo-keyboard': 'no', 'osc': 'no',
                      'sub-auto': 'exact', 'sub-codepage': 'auto', 'keep-open': 'yes',
                      'volume-max': '400', 'hwdec': 'auto-safe',
                      'log-file': str(output.with_suffix('.mpv.log')), 'msg-level': 'all=v'})
    if args.profile == 'analyze1s':
        options.update({'demuxer-lavf-analyzeduration': '1', 'demuxer-lavf-probesize': '1048576'})
    if args.profile == 'lazyindex':
        options['demuxer-lavf-o'] = 'fflags=+ignidx'
    if args.profile in ('smallbuffers', 'winrandomsmall', 'winunbuffered', 'winrandomsmallnoprobe'):
        options.update({'stream-buffer-size': '4096', 'demuxer-lavf-buffersize': '4096'})
    if args.profile == 'winrandomsmallnoprobe':
        options['demuxer-lavf-probe-info'] = 'no'
    report = dict(profile=args.profile, options=options, source=str(source), dll_sha256=pin,
                  source_before_check=before, source_check=args.source_check, finished=False, events=[],
                  scope='Pinned libmpv with null video/audio; no GPU context or physical output. Source hashing warms caches. Diagnostic probe, not application performance acceptance or a replacement player.')

    def save():
        output.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding='utf8')

    try:
        for key, value in options.items():
            assert option(handle, key.encode(), value.encode()) >= 0, key
        assert initialize(handle) >= 0
        custom_transport = args.profile in ('winrandom', 'winnormal', 'winrandomsmall', 'winunbuffered', 'winrandomsmallnoprobe')
        if custom_transport:
            from mpv_winread import register
            transport, transport_lifetime = register(lib, handle, source, args.profile != 'winnormal', args.profile == 'winunbuffered')
            report['transport'] = transport
        report['mpv_version'] = text('mpv-version')
        report['ffmpeg_version'] = text('ffmpeg-version')
        save()
        began = time.monotonic()
        send('loadfile', 'winreadprobe://source.mp4' if custom_transport else str(source), 'replace')
        loaded = False
        while time.monotonic() - began < 45:
            event = wait(handle, .05).contents
            if event.id:
                report['events'].append(dict(id=event.id, error=event.error, ms=(time.monotonic() - began) * 1000))
            if event.id == 8:
                loaded = True
                report['file_loaded_ms'] = (time.monotonic() - began) * 1000
                break
            if event.id == 7 and event.error < 0:
                report['load_error'] = event.error
                break
        report['loaded'] = loaded
        if loaded:
            restart_began = time.monotonic()
            while time.monotonic() - restart_began < 15:
                if wait(handle, .02).contents.id == 21:
                    report['first_playback_restart_ms'] = (time.monotonic() - began) * 1000
                    break
            report['properties'] = {name: text(name) for name in
                ['duration', 'seekable', 'video-codec', 'audio-codec-name', 'video-params/w', 'video-params/h', 'demuxer']}
            send('set', 'pause', 'yes')
            seeks = []
            for target in [float(value) for value in args.seek_targets.split(',')]:
                # time-pos can expose the requested target before decode finishes.
                # Drain old events and require a fresh MPV_EVENT_PLAYBACK_RESTART.
                while wait(handle, 0).contents.id:
                    pass
                start = time.monotonic()
                read_begin = len(report.get('transport', {}).get('reads', []))
                send('seek', str(target), 'absolute+exact')
                actual = None
                restarted = False
                while time.monotonic() - start < 12:
                    event = wait(handle, .02).contents
                    if event.id == 21:
                        restarted = True
                    value = text('time-pos')
                    actual = float(value) if value else None
                    if restarted and actual is not None and abs(actual - target) < .2:
                        break
                seeks.append(dict(target=target, actual=actual, ms=(time.monotonic() - start) * 1000,
                                  playback_restarted=restarted, video_pts=text('video-pts'),
                                  seeking=text('seeking'),
                                  transport_read_begin=read_begin,
                                  transport_read_end=len(report.get('transport', {}).get('reads', [])),
                                  matched=restarted and actual is not None and abs(actual - target) < .2))
            report['seeks'] = seeks
    finally:
        destroy(handle)
        directory.close()
        after_stat = source.stat()
        report['source_after_check'] = source_identity()
        report['checked_source_bytes_unchanged'] = before == report['source_after_check'] and (
            before_stat.st_size, before_stat.st_mtime_ns) == (after_stat.st_size, after_stat.st_mtime_ns)
        report['full_source_hash_verified'] = args.source_check == 'full' and report['checked_source_bytes_unchanged']
        report['finished'] = True
        save()
    print(json.dumps({key: value for key, value in report.items()
                      if key not in ('events', 'options', 'source', 'transport')}, indent=2))
    assert report['checked_source_bytes_unchanged']


if __name__ == '__main__':
    main()
