"""Generated65MP long PNG native viewport pixels; no private source or legacy build."""
from pathlib import Path
import argparse,ctypes,hashlib,json,struct,subprocess,time,uuid,zlib
root=Path(__file__).resolve().parent.parent
p=argparse.ArgumentParser();p.add_argument('--exe',type=Path,required=True);p.add_argument('--prefix',required=True);a=p.parse_args();assert a.prefix.replace('-','').isalnum()
exe=a.exe.resolve(strict=True);sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
public=root/'target'/('long-png-owned-'+uuid.uuid4().hex);(public/'media').mkdir(parents=True);(public/'.long-png-owned').write_text('Generated PNG only; source preserved')
w,h=1741,37528
def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data)&0xffffffff)
rows=[]
for band in range(3):
    for line in range(64):
        base=[35,45,55];base[band]=210
        pixels=bytearray()
        for x in range(w):
            color=[245,245,245] if (x//4+line//4)%2==0 else base
            pixels.extend(color)
        rows.append(b'\0'+pixels)
generated=public/'media/long.png';compress=zlib.compressobj(1)
with generated.open('xb') as f:
    f.write(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,2,0,0,0)))
    for y in range(h):
        data=compress.compress(rows[min(2,y*3//h)*64+y%64])
        if data:f.write(chunk(b'IDAT',data))
    f.write(chunk(b'IDAT',compress.flush())+chunk(b'IEND',b''))

class Memory(ctypes.Structure):
    _fields_=[('cb',ctypes.c_ulong),('PageFaultCount',ctypes.c_ulong)]+[(n,ctypes.c_size_t) for n in ['PeakWorkingSetSize','WorkingSetSize','QuotaPeakPagedPoolUsage','QuotaPagedPoolUsage','QuotaPeakNonPagedPoolUsage','QuotaNonPagedPoolUsage','PagefileUsage','PeakPagefileUsage','PrivateUsage']]
kernel=ctypes.WinDLL('kernel32',use_last_error=True);psapi=ctypes.WinDLL('psapi',use_last_error=True)
kernel.OpenProcess.restype=ctypes.c_void_p;kernel.CloseHandle.argtypes=[ctypes.c_void_p]
psapi.GetProcessMemoryInfo.argtypes=[ctypes.c_void_p,ctypes.POINTER(Memory),ctypes.c_ulong]
def run(program,task,name,data_override=None):
    work=task/name;work.mkdir();data=data_override or task/(name+'-data');output=work/'evidence';output.mkdir()
    before=sha(task/'media/long.png');peak=0
    with (work/'stdout.log').open('wb') as stdout,(work/'stderr.log').open('wb') as stderr:
        app=subprocess.Popen([str(program),'--format-smoke',str(output),'--open',str(task/'media'),'--data-dir',str(data),'--instance-scope',task.name+'-'+name,'--quit-after','90000'],cwd=work,stdout=stdout,stderr=stderr)
        handle=kernel.OpenProcess(0x410,False,app.pid);began=time.monotonic()
        try:
            while app.poll() is None:
                if time.monotonic()-began>100:raise TimeoutError(f'Owned diagnostic still live PID {app.pid}; inspect before retry')
                m=Memory();m.cb=ctypes.sizeof(m)
                if handle and psapi.GetProcessMemoryInfo(handle,ctypes.byref(m),m.cb):peak=max(peak,m.PeakWorkingSetSize,m.WorkingSetSize)
                time.sleep(.15)
        finally:
            if handle:kernel.CloseHandle(handle)
    report=json.loads((output/'report.json').read_text(encoding='utf-8'))
    assert sha(task/'media/long.png')==before
    return report,dict(exit_code=app.returncode,exe_sha256=sha(program),peak_working_set_mib=peak/1048576,source_unchanged=True),output,data


original=sha(generated)
results=[]
ff=root/'vendor/runtime/ffmpeg.exe'
def rgba(path,filter=None):
    cmd=[str(ff),'-v','error','-threads','1','-i',str(path)]
    if filter:cmd+=['-vf',filter]
    return subprocess.check_output(cmd+['-f','rawvideo','-pix_fmt','rgba','-'],creationflags=subprocess.CREATE_NO_WINDOW)
for task,label in [(public,'generated')]:
    raw,r,output,data=run(exe,task,'native')
    assert r['exit_code']==0 and raw['error'] is None,(label,raw['error'])
    phases={v['phase']:v for v in raw['rows']}
    assert all(not v['failures'] for v in raw['rows'])
    assert all((v['record']['width'],v['record']['height'])==(w,h) for v in raw['rows'])
    checked=[]
    for phase in ['detail_zoom','detail_top','detail_bottom']:
        v=phases[phase];region=v['image_viewport'];assert v['image_viewport_ready'] and region
        assert region['width']*region['height']<=16_777_216
        expected=rgba(task/'media/long.png',f"crop={region['width']}:{region['height']}:{region['x']}:{region['y']}")
        candidates=[]
        for png in data.rglob('*.png'):
            b=png.read_bytes()[:24]
            if len(b)==24 and struct.unpack('>II',b[16:24])==(region['width'],region['height']):candidates.append(png)
        assert any(rgba(png)==expected for png in candidates),(label,phase,'No exact native-resolution crop in cache')
        checked.append(dict(phase=phase,region=region,exact_source_pixels=True))
    assert phases['detail_top']['image_viewport']['y']==0
    last=phases['detail_bottom']['image_viewport'];assert last['y']+last['height']==h
    r.update(label=label,checks_pass=True,dimensions=[w,h],phases=list(phases),native_regions=checked,texture_bytes_max=max(v['texture_bytes'] for v in raw['rows']))
    if label=='generated':r.update(public_evidence=str(output),public_work=str(task/'native'))
    else:r['private_evidence_not_exported']=True
    results.append(r);print('PASS '+label,flush=True)

assert sha(generated)==original
out=root/'artifacts'/(a.prefix+'-long-png-native.json');assert not out.exists()
out.write_text(json.dumps(dict(checks_pass=True,exe_sha256=sha(exe),source_sha256_before=original,source_sha256_after=sha(generated),source_unchanged=True,rows=results,scope='Generated1741x37528 RGB PNG, native list/fit/native-size/top/bottom/return; exact viewport pixels vs locked FFmpeg, bounded output. No private source, no legacy build or failed-index migration claim; no physical input/full performance acceptance.'),ensure_ascii=False,indent=2)+'\n')
print('Generated long PNG native checks passed',flush=True)
