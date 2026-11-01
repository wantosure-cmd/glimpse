"""Generate isolated synthetic media, verify native slideshow and preserve sources."""
from pathlib import Path
import argparse, hashlib, json, struct, subprocess, uuid, zlib
from package_local import production_inputs
ROOT = Path(__file__).resolve().parent.parent
p = argparse.ArgumentParser()
p.add_argument('--exe', type=Path, required=True)
p.add_argument('--prefix', default='v030-presentation')
a = p.parse_args()
assert a.prefix.replace('-', '').isalnum()
exe = a.exe.resolve(strict=True)
task = ROOT/'target'/('presentation-owned-'+uuid.uuid4().hex)
media, work = task/'media', task/'work'
media.mkdir(parents=True); work.mkdir()
def chunk(k, d):
    return struct.pack('>I',len(d))+k+d+struct.pack('>I',zlib.crc32(k+d)&0xffffffff)
for i,(w,h) in enumerate([(900,1200),(1200,800),(800,1200),(1200,700),(1000,1000),(1200,800),(800,1200),(1200,900)]):
    n=i+1 if i<5 else i+2
    data=b''.join(b'\0'+b''.join(bytes(((x*180//w+25+i*20)%230,(y*180//h+25+i*15)%230,90+i*16)) for x in range(w)) for y in range(h))
    (media/f'{n:02d}-photo.png').write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(data))+chunk(b'IEND',b''))
ffmpeg=next(f for f in (ROOT/'vendor/runtime/ffmpeg.exe',exe.parent/'runtime/ffmpeg.exe') if f.is_file())
subprocess.run([str(ffmpeg),'-v','error','-f','lavfi','-i','testsrc2=size=640x360:rate=24','-f','lavfi','-i','sine=frequency=440','-t','3','-c:v','mpeg4','-c:a','aac',str(media/'06-video.mov')],check=True,creationflags=subprocess.CREATE_NO_WINDOW)
def sha(f): return hashlib.sha256(f.read_bytes()).hexdigest()
def hashes(): return {f.name:sha(f) for f in media.iterdir()}
before=hashes(); source=production_inputs(); exe_sha=sha(exe)
out=ROOT/'artifacts'/(a.prefix+'-raw.json')
with (task/'stdout.log').open('wb') as stdout,(task/'stderr.log').open('wb') as stderr:
    proc=subprocess.Popen([str(exe),'--presentation-smoke',str(out),'--open',str(media),'--data-dir',str(task/'data'),'--instance-scope',task.name],cwd=work,stdout=stdout,stderr=stderr)
    code=proc.wait(timeout=90)
raw=json.loads(out.read_text(encoding='utf-8'))
result=dict(checks_pass=code==0 and raw['checks_pass'] and before==hashes() and source==production_inputs() and exe_sha==sha(exe),exit_code=code,exe_sha256=exe_sha,production_source_sha256=source,work=str(work),raw=str(out),sources_unchanged=before==hashes(),scope=raw['scope'])
(ROOT/'artifacts'/(a.prefix+'-verification.json')).write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({k:v for k,v in result.items() if k!='production_source_sha256'},ensure_ascii=False,indent=2))
assert result['checks_pass'],raw
