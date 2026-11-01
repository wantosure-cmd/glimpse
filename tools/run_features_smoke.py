from pathlib import Path
import argparse,hashlib,json,struct,subprocess,uuid,zlib
from package_local import production_inputs
ROOT=Path(__file__).resolve().parent.parent
p=argparse.ArgumentParser();p.add_argument('--exe',type=Path,required=True);p.add_argument('--prefix',default='v040-features');a=p.parse_args();assert a.prefix.replace('-','').isalnum()
task=ROOT/'target'/('features-owned-'+uuid.uuid4().hex);media=task/'media';work=task/'work';media.mkdir(parents=True);work.mkdir();(media/'empty/nested').mkdir(parents=True);(media/'with-file').mkdir();(media/'with-file/keep.txt').write_text('must remain',encoding='utf-8');(media/'generated-fixture.marker').write_text('dedicated disposable generated media',encoding='utf-8')
def chunk(k,d):return struct.pack('>I',len(d))+k+d+struct.pack('>I',zlib.crc32(k+d)&0xffffffff)
for i in range(6):
 w,h=(600,400) if i%2 else (500,600);data=b''.join(b'\0'+bytes((40+i*30,90+y*100//h,130))*w for y in range(h));(media/f'{i+1:02}-photo.png').write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(data))+chunk(b'IEND',b''))
subprocess.run([str(ROOT/'vendor/runtime/ffmpeg.exe'),'-v','error','-f','lavfi','-i','testsrc2=size=640x360:rate=24','-f','lavfi','-i','sine=frequency=440','-t','8','-c:v','mpeg4','-c:a','aac',str(media/'07-video.mov')],check=True,creationflags=subprocess.CREATE_NO_WINDOW)
def fixture_hashes():return {f.relative_to(media).as_posix():hashlib.sha256(f.read_bytes()).hexdigest() for f in sorted(media.rglob('*')) if f.is_file()}
fixture_before=fixture_hashes()
exe=a.exe.resolve(strict=True);output=ROOT/'artifacts'/(a.prefix+'-raw.json');apphash=hashlib.sha256(exe.read_bytes()).hexdigest();production=production_inputs()
with (task/'stdout.log').open('wb')as out,(task/'stderr.log').open('wb')as err:
 proc=subprocess.Popen([str(exe),'--features-smoke',str(output),'--open',str(media),'--data-dir',str(task/'data'),'--instance-scope',task.name],cwd=work,stdout=out,stderr=err)
 code=proc.wait(timeout=115)
raw=json.loads(output.read_text(encoding='utf-8')) if output.exists() else {'checks_pass':False,'error':'No report'}
result=dict(checks_pass=code==0 and raw['checks_pass'] and production==production_inputs() and apphash==hashlib.sha256(exe.read_bytes()).hexdigest(),exit_code=code,raw=raw,task=str(task),exe_sha256=apphash,production_source_sha256=production,fixture_before_sha256=fixture_before,fixture_after_sha256=fixture_hashes())
(ROOT/'artifacts'/(a.prefix+'-verification.json')).write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8');print(json.dumps(result,ensure_ascii=False,indent=2));assert result['checks_pass']
