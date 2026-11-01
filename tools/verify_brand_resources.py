"""Read PE resources as data; never install or launch the executable."""
from pathlib import Path
import ctypes,struct,hashlib,json,argparse
ROOT=Path(__file__).resolve().parent.parent
p=argparse.ArgumentParser();p.add_argument('--app',type=Path,default=ROOT/'target/release/media-browser.exe');p.add_argument('--setup',type=Path,default=ROOT/'target/setup-static/release/media-browser-setup.exe');p.add_argument('--output',type=Path,default=ROOT/'artifacts/glimpse-final-brand-resources.json');a=p.parse_args()
k=ctypes.WinDLL('kernel32',use_last_error=True)
k.LoadLibraryExW.argtypes=[ctypes.c_wchar_p,ctypes.c_void_p,ctypes.c_uint32];k.LoadLibraryExW.restype=ctypes.c_void_p
k.FindResourceW.argtypes=[ctypes.c_void_p,ctypes.c_void_p,ctypes.c_void_p];k.FindResourceW.restype=ctypes.c_void_p
k.LoadResource.argtypes=[ctypes.c_void_p,ctypes.c_void_p];k.LoadResource.restype=ctypes.c_void_p
k.LockResource.argtypes=[ctypes.c_void_p];k.LockResource.restype=ctypes.c_void_p
k.SizeofResource.argtypes=[ctypes.c_void_p,ctypes.c_void_p];k.SizeofResource.restype=ctypes.c_uint32
k.FreeLibrary.argtypes=[ctypes.c_void_p]
rows=[]
for exe in (a.app,a.setup):
    module=k.LoadLibraryExW(str(exe.resolve()),None,2|32);assert module,ctypes.get_last_error()
    try:
        def resource(name,kind):
            handle=k.FindResourceW(module,ctypes.c_void_p(name),ctypes.c_void_p(kind));assert handle,(exe,name,kind,ctypes.get_last_error())
            return ctypes.string_at(k.LockResource(k.LoadResource(module,handle)),k.SizeofResource(module,handle))
        data=resource(1,14);reserved,kind,count=struct.unpack('<HHH',data[:6]);assert (reserved,kind,count)==(0,1,7)
        icons=[]
        for i in range(count):
            w,h,colors,unused,planes,bits,size,ident=struct.unpack('<BBBBHHIH',data[6+i*14:20+i*14]);payload=resource(ident,3)
            assert len(payload)==size and payload.startswith(b'\x89PNG')
            icons.append(dict(width=w or 256,height=h or 256,sha256=hashlib.sha256(payload).hexdigest()))
        assert icons[-1]['sha256']==hashlib.sha256((ROOT/'assets/brand/glimpse.png').read_bytes()).hexdigest()
        assert 'Glimpse'.encode('utf-16-le') in resource(1,16)
        rows.append(dict(product_name_marker=True,exe=str(exe),exe_sha256=hashlib.sha256(exe.read_bytes()).hexdigest(),icons=icons))
    finally:k.FreeLibrary(module)
result=dict(checks_pass=True,rows=rows,scope='PE icon group and PNG resources read as data; not Explorer icon-cache or taskbar visual verification')
a.output.write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8');print(json.dumps(result,ensure_ascii=False))
