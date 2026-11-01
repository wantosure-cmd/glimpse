"""Read-only native design check, in an isolated data directory and owned fixture."""
from pathlib import Path
import argparse, hashlib, json, shutil, subprocess, uuid, zlib, struct
ROOT = Path(__file__).resolve().parent.parent
p = argparse.ArgumentParser()
p.add_argument('--exe', type=Path, default=ROOT/'target/debug/media-browser.exe')
p.add_argument('--prefix', default='raycast-design')
a = p.parse_args()
assert a.prefix.replace('-', '').isalnum()
task = ROOT/'target'/('design-owned-'+uuid.uuid4().hex)
media = task/'media'
media.mkdir(parents=True)
work = task/'work'
work.mkdir()
# Public/synthetic test files only. Never screenshot a private photo collection.
public = ROOT/'dist/0.1.0-local-test-20261021-compact-sizing/verification/independent-source/public-heic.png'
video = ROOT/'test-media/test-h264-aac.mov'
# Source-only checkouts can create their own fixture; no older delivery is required.
if not public.is_file():
    public=task/'generated.png'
    def chunk(kind,data):
        return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data)&0xffffffff)
    pixels=b''.join(b'\0'+b''.join(bytes((40+x*100//640,65+y*90//480,120)) for x in range(640)) for y in range(480))
    public.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',640,480,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(pixels))+chunk(b'IEND',b''))
if not video.is_file():
    ffmpeg=next((f for f in (ROOT/'vendor/runtime/ffmpeg.exe',a.exe.resolve().parent/'runtime/ffmpeg.exe') if f.is_file()),None)
    assert ffmpeg is not None,'Restore the pinned runtime before running native diagnostics'
    video=task/'generated.mov'
    subprocess.run([str(ffmpeg),'-v','error','-f','lavfi','-i','testsrc2=size=640x360:rate=24','-f','lavfi','-i','sine=frequency=440','-t','3','-c:v','mpeg4','-c:a','aac',str(video)],check=True,creationflags=subprocess.CREATE_NO_WINDOW)
for folder in ('', 'Travel', 'Travel/Coast', 'Studio'):
    d = media/folder
    d.mkdir(exist_ok=True, parents=True)
    shutil.copyfile(public, d/('01 - '+'长文件名'*12+'.png' if not folder else '01-photo.png'))
    shutil.copyfile(video, d/'02-video.mov')
def hashes():
    return {str(f.relative_to(media)):hashlib.sha256(f.read_bytes()).hexdigest() for f in media.rglob('*') if f.is_file()}
before=hashes()
exe=a.exe.resolve(strict=True)
out=ROOT/'artifacts'/(a.prefix+'-raw.json')
with (task/'stdout.log').open('wb') as stdout, (task/'stderr.log').open('wb') as stderr:
    proc=subprocess.Popen([str(exe),'--design-smoke',str(out),'--open',str(media),'--data-dir',str(task/'data'),'--instance-scope',task.name],cwd=work,stdout=stdout,stderr=stderr)
    code=proc.wait(timeout=110)
raw=json.loads(out.read_text(encoding='utf-8'))
pixel_checks = {}
if raw['checks_pass']:
    rows={r['name']:r for r in raw['rows']}
    ffmpeg=next((f for f in (ROOT/'vendor/runtime/ffmpeg.exe',exe.parent/'runtime/ffmpeg.exe') if f.is_file()),None)
    colors={}
    for name in ('video-restored','video-right-90','video-left-270'):
        row=rows[name];png=work/'artifacts'/('design-'+name+'.png')
        w,h=struct.unpack('>II',png.read_bytes()[16:24])
        data=subprocess.check_output([str(ffmpeg),'-v','error','-i',str(png),'-f','rawvideo','-pix_fmt','rgb24','-'],creationflags=subprocess.CREATE_NO_WINDOW)
        x,y,rw,rh=[v*row['pixels_per_point'] for v in row['video_region']]
        samples=[]
        for rx,ry in ((.25,.25),(.75,.25),(.25,.75),(.75,.75)):
            patches=[data[(int(y+(ry+dy/1000)*rh)*w+int(x+(rx+dx/1000)*rw))*3:][:3] for dy in range(-15,16,3) for dx in range(-15,16,3)]
            samples.append([sum(p[k] for p in patches)/len(patches) for k in range(3)])
        colors[name]=samples
    for name,order in [('video-right-90',[2,0,3,1]),('video-left-270',[1,3,0,2])]:
        delta=max(abs(a-b) for sample,index in zip(colors[name],order) for a,b in zip(sample,colors['video-restored'][index]))
        pixel_checks[name]=dict(max_channel_difference=delta,passed=delta<25)
    assert all(r['passed'] for r in pixel_checks.values()),pixel_checks
result=dict(checks_pass=code==0 and raw['checks_pass'] and before==hashes(),exit_code=code,exe_sha256=hashlib.sha256(exe.read_bytes()).hexdigest(),work=str(work),raw=str(out),sources_unchanged=before==hashes(),scope=raw['scope'],rotation_pixels=pixel_checks)
(ROOT/'artifacts'/(a.prefix+'-verification.json')).write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps(result,ensure_ascii=False,indent=2))
assert result['checks_pass'],raw
