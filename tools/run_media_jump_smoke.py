"""Read-only native media-location jump test using generated/owned files only."""
from pathlib import Path
import argparse,json,hashlib,uuid,shutil,subprocess,struct,zlib
ROOT=Path(__file__).resolve().parent.parent
p=argparse.ArgumentParser();p.add_argument('--exe',type=Path,required=True);p.add_argument('--prefix',required=True);a=p.parse_args()
assert a.prefix.replace('-','').isalnum();out=ROOT/'artifacts'/(a.prefix+'.json');assert not out.exists()
task=ROOT/'target'/('media-jump-owned-'+uuid.uuid4().hex);media=task/'media';media.mkdir(parents=True);work=task/'work';work.mkdir();evidence=task/'native';evidence.mkdir()
def chunk(k,b):return struct.pack('>I',len(b))+k+b+struct.pack('>I',zlib.crc32(k+b)&0xffffffff)
def png(path):
    path.parent.mkdir(parents=True,exist_ok=True);w,h=640,360
    pixels=b''.join(b'\0'+bytes((60+y%120,90,170))*w for y in range(h))
    path.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(pixels))+chunk(b'IEND',b''))
png(media/'00-images/one.png');png(media/'04-images/two.png')
for folder in ['01-gap','03-gap','06-tail']:
    for i in range(40):(media/folder/f'{i:03}-empty').mkdir(parents=True)
deep=media/'05-deep'
for i in range(12):deep/=f'层级-{i:02}'
png(deep/'deep.png')
video=media/'02-video/one.mov';video.parent.mkdir()
source=ROOT/'test-media/test-h264-aac.mov'
if source.exists():shutil.copyfile(source,video)
else:
    subprocess.run([str(ROOT/'vendor/runtime/ffmpeg.exe'),'-v','error','-f','lavfi','-i','testsrc2=size=640x360:rate=24','-t','3','-c:v','mpeg4',str(video)],check=True,creationflags=subprocess.CREATE_NO_WINDOW)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
before={str(p.relative_to(media)):sha(p) for p in media.rglob('*') if p.is_file()};exe=a.exe.resolve(strict=True)
with (work/'stdout.log').open('wb') as stdout,(work/'stderr.log').open('wb') as stderr:
    app=subprocess.Popen([str(exe),'--media-jump-smoke',str(evidence),'--open',str(media),'--data-dir',str(task/'data'),'--instance-scope',task.name],cwd=work,stdout=stdout,stderr=stderr);code=app.wait(timeout=125)
raw=json.loads((evidence/'report.json').read_text());rows={r['name']:r for r in raw['rows']}
required=['down-empty-gap','down-image-visible','up-target-video','up-video-visible','collapsed-target-unfolded','images-filter-skips-video','videos-end-disabled','collapsed-ancestor-unfolded','narrow-long-breadcrumb']
unchanged=before=={str(p.relative_to(media)):sha(p) for p in media.rglob('*') if p.is_file()}
result=dict(checks_pass=code==0 and raw['checks_pass'] and all(n in rows for n in required) and unchanged,
    exe_sha256=sha(exe),exit_code=code,source_unchanged=unchanged,source_hashes=before,native=str(evidence),work=str(work),scope=raw['scope'],error=raw['error'])
out.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n');print(json.dumps(result,ensure_ascii=False,indent=2));assert result['checks_pass'],raw
