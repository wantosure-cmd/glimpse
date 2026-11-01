"""Read PE import DLL names using only Python's standard library (never load DLLs)."""
import struct
from pathlib import Path


def imports(path: Path) -> list[str]:
    data = path.read_bytes()
    if data[:2] != b'MZ':
        raise ValueError('Not a PE file')
    pe, = struct.unpack_from('<I', data, 0x3c)
    if data[pe:pe+4] != b'PE\0\0':
        raise ValueError('Missing PE signature')
    sections, = struct.unpack_from('<H', data, pe+6)
    opt_size, = struct.unpack_from('<H', data, pe+20)
    optional = pe+24
    magic, = struct.unpack_from('<H', data, optional)
    dirs = {0x10b: 96, 0x20b: 112}.get(magic)
    if dirs is None:
        raise ValueError('Unknown PE optional header')
    import_rva, import_size = struct.unpack_from('<II', data, optional+dirs+8)
    table = optional+opt_size

    def offset(rva):
        for i in range(sections):
            virtual_size, virtual_address, size, raw = struct.unpack_from('<IIII', data, table+i*40+8)
            if virtual_address <= rva < virtual_address+max(virtual_size, size):
                return raw+rva-virtual_address
        raise ValueError('RVA outside PE sections')

    if not import_rva:
        return []
    result = []
    base = offset(import_rva)
    for pos in range(base, base+import_size, 20):
        fields = struct.unpack_from('<IIIII', data, pos)
        if not any(fields):
            break
        start = offset(fields[3])
        end = data.index(b'\0', start, start+512)
        result.append(data[start:end].decode('ascii'))
    return result
