"""Diagnostic-only read-only Win32 transport for the locked mpv stream_cb ABI."""
import ctypes as C
import time


def register(lib, mpv, source, random_access, unbuffered=False):
    kernel = C.WinDLL('kernel32', use_last_error=True)

    def bind(name, result, *params):
        fn = getattr(kernel, name)
        fn.restype, fn.argtypes = result, list(params)
        return fn

    create = bind('CreateFileW', C.c_void_p, C.c_wchar_p, C.c_uint32, C.c_uint32,
                  C.c_void_p, C.c_uint32, C.c_uint32, C.c_void_p)
    read = bind('ReadFile', C.c_int, C.c_void_p, C.c_void_p, C.c_uint32,
                C.POINTER(C.c_uint32), C.c_void_p)
    seek = bind('SetFilePointerEx', C.c_int, C.c_void_p, C.c_int64,
                C.POINTER(C.c_int64), C.c_uint32)
    size = bind('GetFileSizeEx', C.c_int, C.c_void_p, C.POINTER(C.c_int64))
    close = bind('CloseHandle', C.c_int, C.c_void_p)
    cancel = bind('CancelIoEx', C.c_int, C.c_void_p, C.c_void_p)
    allocate = bind('VirtualAlloc', C.c_void_p, C.c_void_p, C.c_size_t, C.c_uint32, C.c_uint32)
    free = bind('VirtualFree', C.c_int, C.c_void_p, C.c_size_t, C.c_uint32)
    Read = C.CFUNCTYPE(C.c_int64, C.c_void_p, C.c_void_p, C.c_uint64)
    Seek = C.CFUNCTYPE(C.c_int64, C.c_void_p, C.c_int64)
    Size = C.CFUNCTYPE(C.c_int64, C.c_void_p)
    Close = C.CFUNCTYPE(None, C.c_void_p)
    Cancel = C.CFUNCTYPE(None, C.c_void_p)

    class Info(C.Structure):
        _fields_ = [('cookie', C.c_void_p), ('read_fn', Read), ('seek_fn', Seek),
                    ('size_fn', Size), ('close_fn', Close), ('cancel_fn', Cancel)]

    Open = C.CFUNCTYPE(C.c_int, C.c_void_p, C.c_char_p, C.POINTER(Info))
    state = dict(flags='FILE_FLAG_NO_BUFFERING' if unbuffered else ('FILE_FLAG_RANDOM_ACCESS' if random_access else 'FILE_ATTRIBUTE_NORMAL'),
                 reads=[], seeks=[], errors=[], opens=0, closes=0)
    handles = {}
    path = str(source)
    if path.startswith('\\\\') and not path.startswith('\\\\?\\'):
        path = '\\\\?\\UNC\\' + path[2:]
    elif not path.startswith('\\\\?\\'):
        path = '\\\\?\\' + path

    @Read
    def on_read(handle, buffer, count):
        row = handles[handle]
        if row['cancelled']:
            return -1
        got = C.c_uint32()
        start = time.monotonic()
        physical_offset = row['offset']
        if unbuffered:
            # All physical requests are page aligned and bounded to ~1MiB.
            # Logical reads/seeks still expose the original unmodified file.
            physical_offset = row['offset'] // 4096 * 4096
            skip = row['offset'] - physical_offset
            wanted = min(count, 1048576)
            aligned_size = (skip + wanted + 4095) // 4096 * 4096
            memory = allocate(None, aligned_size, 0x3000, 0x04)
            if not memory:
                state['errors'].append(dict(operation='allocate', code=C.get_last_error()))
                return -1
            try:
                position = C.c_int64()
                ok = seek(handle, physical_offset, C.byref(position), 0)
                if ok:
                    ok = read(handle, memory, aligned_size, C.byref(got), None)
                error = C.get_last_error() if not ok else 0
                physical_bytes = got.value
                got.value = min(wanted, max(0, got.value - skip))
                if ok and got.value:
                    C.memmove(buffer, memory + skip, got.value)
            finally:
                free(memory, 0, 0x8000)
        else:
            ok = read(handle, buffer, min(count, 2147483647), C.byref(got), None)
            error = C.get_last_error() if not ok else 0
            physical_bytes = got.value
        state['reads'].append(dict(offset=row['offset'], requested=count, bytes=got.value,
                                   physical_offset=physical_offset, physical_bytes=physical_bytes,
                                   ms=(time.monotonic() - start) * 1000))
        if not ok:
            state['errors'].append(dict(operation='read', code=error))
            return -1
        row['offset'] += got.value
        return got.value

    @Seek
    def on_seek(handle, offset):
        if handles[handle]['cancelled']:
            return -1
        position = C.c_int64()
        if not seek(handle, offset // 4096 * 4096 if unbuffered else offset, C.byref(position), 0):
            state['errors'].append(dict(operation='seek', code=C.get_last_error()))
            return -1
        handles[handle]['offset'] = offset
        state['seeks'].append(offset)
        return offset

    @Size
    def on_size(handle):
        value = C.c_int64()
        return value.value if size(handle, C.byref(value)) else -1

    @Close
    def on_close(handle):
        close(handle)
        handles.pop(handle, None)
        state['closes'] += 1

    @Cancel
    def on_cancel(handle):
        if handle in handles:
            handles[handle]['cancelled'] = True
            cancel(handle, None)

    @Open
    def on_open(user, uri, info):
        if uri != b'winreadprobe://source.mp4':
            return -13
        handle = create(path, 0x80000000, 7, None, 3,
                        0x20000000 if unbuffered else (0x10000000 if random_access else 0x80), None)
        if handle in (None, C.c_void_p(-1).value):
            state['errors'].append(dict(operation='open', code=C.get_last_error()))
            return -13
        handles[handle] = dict(offset=0, cancelled=False)
        state['opens'] += 1
        info[0] = Info(handle, on_read, on_seek, on_size, on_close, on_cancel)
        return 0

    add = lib.mpv_stream_cb_add_ro
    add.restype, add.argtypes = C.c_int, [C.c_void_p, C.c_char_p, C.c_void_p, Open]
    assert add(mpv, b'winreadprobe', None, on_open) >= 0
    # Keep every callback alive until mpv_terminate_destroy returns.
    return state, (on_open, on_read, on_seek, on_size, on_close, on_cancel, kernel)
