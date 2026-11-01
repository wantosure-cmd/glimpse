"""Generate owned GIF/APNG sources and exercise native visible autoplay/detail playback."""
from pathlib import Path
import argparse, hashlib, json, shutil, struct, subprocess, uuid, zlib, time
from PIL import Image
ROOT = Path(__file__).resolve().parent.parent
p = argparse.ArgumentParser()
p.add_argument('--exe', type=Path, required=True)
p.add_argument('--prefix', required=True)
a = p.parse_args()
assert a.prefix.replace('-', '').isalnum()
output = ROOT/'artifacts'/(a.prefix+'.json')
assert not output.exists()
task = ROOT/'target'/('animation-owned-'+uuid.uuid4().hex)
media = task/'media'; media.mkdir(parents=True)
(media/'empty').mkdir(); work=task/'work'; (work/'artifacts').mkdir(parents=True)
def chunk(kind,data):
    return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data)&0xffffffff)
def apng(path, frames, plays, cover=False):
    data=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',4,2,8,6,0,0,0))+chunk(b'acTL',struct.pack('>II',len(frames),plays))
    if cover: data+=chunk(b'IDAT',zlib.compress((b'\0'+bytes((255,255,0,255))*4)*2))
    seq=0
    for i,(w,h,x,y,color,n,d,dispose,blend) in enumerate(frames):
        data+=chunk(b'fcTL',struct.pack('>IIIIIHHBB',seq,w,h,x,y,n,d,dispose,blend));seq+=1
        pixels=zlib.compress((b'\0'+bytes(color)*w)*h)
        if i==0 and not cover: data+=chunk(b'IDAT',pixels)
        else: data+=chunk(b'fdAT',struct.pack('>I',seq)+pixels);seq+=1
    path.write_bytes(data+chunk(b'IEND',b''))
for dest,source in [('01-loop.gif','循环.gif'),('02-loop.gif','循环.gif'),('03-finite.gif','有限.gif'),('04-once.gif','单次.gif')]:
    original=ROOT/'test-media/gif-demo'/source
    if original.is_file():
        shutil.copyfile(original,media/dest)
    else:
        # Source-only archives intentionally omit test-media. Generate the
        # same three-color timing/count fixture instead of needing a gallery.
        frames=[Image.new('RGB',(2,1),c) for c in [(255,0,0),(0,255,0),(0,0,255)]]
        repeat={} if source=='单次.gif' else {'loop':0 if source=='循环.gif' else 1}
        frames[0].save(media/dest,save_all=True,append_images=frames[1:],duration=[100,350,80],optimize=False,**repeat)
rgb=[(4,2,0,0,(255,0,0,255),1,10,0,0),(4,2,0,0,(0,255,0,255),7,20,0,0),(4,2,0,0,(0,0,255,255),2,25,0,0)]
apng(media/'05-loop.png',rgb,0)
apng(media/'06-finite.apng',[
    (4,2,0,0,(255,0,0,255),1,10,0,0),
    (2,2,0,0,(0,255,0,128),7,20,2,1),
    (2,2,2,0,(0,0,255,255),2,25,1,0),
    (1,1,0,0,(0,255,0,255),17,100,0,0)],2)
apng(media/'07-cover.png',rgb,1,True)
Image.new('RGB',(320,180),(74,96,128)).save(media/'08-static.png')
for i in range(60):
    Image.new('RGB',(320,180),(40+i,70,120)).save(media/f'10-still-{i:03}.png')
sha=lambda f:hashlib.sha256(f.read_bytes()).hexdigest()
before={str(f.relative_to(media)):sha(f) for f in media.rglob('*') if f.is_file()}
rawpath=task/'native'; exe=a.exe.resolve(strict=True)
with (work/'stdout.log').open('wb') as stdout,(work/'stderr.log').open('wb') as stderr:
    proc=subprocess.Popen([str(exe),'--animation-smoke',str(rawpath),'--open',str(media),'--data-dir',str(task/'data'),'--instance-scope',task.name],cwd=work,stdout=stdout,stderr=stderr)
    code=proc.wait(timeout=90)
raw=json.loads((rawpath/'report.json').read_text(encoding='utf8'))
result=dict(checks_pass=False,exe_sha256=sha(exe),exit_code=code,work=str(work),media=str(media),native_report=str(rawpath/'report.json'),scope=raw['scope'],error=raw['error'])
try:
    assert code==0 and raw['error'] is None,raw
    rows={r['name']:r for r in raw['rows'] if r['name']!='list-frame'}
    required=['list-auto-complete','offscreen-released','reentry-playing','apng-infinite','apng-finite','apng-cover-excluded','static-png','navigation-released']
    assert all(n in rows for n in required)
    for n in required:
        assert rows[n]['focused'] is False and rows[n]['pointer'] is None
    off=rows['offscreen-released']
    assert off['list']==[] and off['released'] and off['list_texture_bytes']==0
    assert off['stats']['sources']==0 and off['stats']['canvas_bytes']==0 and off['stats']['ready_bytes']==0
    assert rows['navigation-released']['list']==[]
    pixels={}
    for prefix in ['list-color','apng-color']:
        for i,color in enumerate([(255,0,0),(0,255,0),(0,0,255)]):
            name=f'{prefix}-{i}';row=rows[name]
            image=Image.open(work/'artifacts'/f'animation-{name}.png').convert('RGB')
            if prefix=='list-color':
                entry=next(e for e in row['list'] if e['sequence']>=3 and e['sequence']%3==i and not e['ended'])
                x,y,w,h=entry['rect'];scale=image.width/row.get('ui_width',image.width)
                point=(round((x+w*.5)*scale),round((y+h*.5)*scale))
            else: point=(image.width//2,image.height//2)
            actual=image.getpixel(point);pixels[name]=dict(actual=actual,expected=color,point=point)
            assert max(abs(x-y) for x,y in zip(actual,color))<10,pixels[name]
    assert before=={str(f.relative_to(media)):sha(f) for f in media.rglob('*') if f.is_file()}
    result.update(checks_pass=True,sources_unchanged=True,source_hashes=before,pixel_checks=pixels)
except (AssertionError,KeyError,ValueError,StopIteration) as exc:
    result['failure']=str(exc)
output.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(json.dumps(result,ensure_ascii=False,indent=2))
assert result['checks_pass']
