"""Generated subtree/default-preference and actual wide scrollbar native test."""
from pathlib import Path
import argparse,json,hashlib,subprocess,uuid,struct,zlib
from PIL import Image
ROOT=Path(__file__).resolve().parent.parent
p=argparse.ArgumentParser();p.add_argument('--exe',type=Path,required=True);p.add_argument('--prefix',required=True);a=p.parse_args();assert a.prefix.replace('-','').isalnum()
out=ROOT/'artifacts'/(a.prefix+'.json');assert not out.exists()
task=ROOT/'target'/('empty-fold-owned-'+uuid.uuid4().hex);media=task/'media';media.mkdir(parents=True);(media/'.generated-empty-fold').write_text('Generated files only')
def chunk(k,b):return struct.pack('>I',len(b))+k+b+struct.pack('>I',zlib.crc32(k+b)&0xffffffff)
w,h=64,160;pixels=b''.join(b'\0'+bytes((40+y%150,90,150))*w for y in range(h));data=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(pixels))+chunk(b'IEND',b'')
album=media/'00-media/has-media/album';album.mkdir(parents=True)
for i in range(120):(album/f'{i:03}.png').write_bytes(data)
(media/'01-empty-app/_internal/vendored/requests/packages/certifi').mkdir(parents=True)
for i in range(25):(media/'02-empty-branch'/f'{i:03}-empty').mkdir(parents=True)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
before={str(p.relative_to(media)):sha(p) for p in media.rglob('*') if p.is_file()};exe=a.exe.resolve(strict=True);runs=[]
for disabled in [False,True]:
    work=task/('restart' if disabled else 'first');work.mkdir();evidence=work/'evidence';evidence.mkdir()
    args=[str(exe),'--empty-fold-smoke',str(evidence),'--open',str(media),'--data-dir',str(task/'data'),'--instance-scope',task.name]
    if disabled:args.append('--verify-fold-disabled')
    with (work/'stdout.log').open('wb') as stdout,(work/'stderr.log').open('wb') as stderr:
        app=subprocess.Popen(args,cwd=work,stdout=stdout,stderr=stderr);code=app.wait(timeout=135)
    raw=json.loads((evidence/'report.json').read_text());runs.append(dict(exit_code=code,evidence=str(evidence),checks_pass=raw['checks_pass'],error=raw['error']))
    if code!=0 or not raw['checks_pass']:break
result=dict(checks_pass=False,exe_sha256=sha(exe),runs=runs,fixture=str(media),source_unchanged=before=={str(p.relative_to(media)):sha(p) for p in media.rglob('*') if p.is_file()},scope='Native controlled input/FBO and restart; generated sources. Real scrollbar handle drag, rendered handle dimensions, manual auto-fold override/resize, actual settings checkbox and persistence. No physical mouse/full performance certification.')
try:
    assert len(runs)==2 and all(r['checks_pass'] and r['exit_code']==0 for r in runs),runs
    assert json.loads((task/'data/settings.json').read_text())['auto_fold_empty'] is False
    raw=json.loads((Path(runs[0]['evidence'])/'report.json').read_text());rows={r['name']:r for r in raw['rows']}
    assert rows['scrollbar-dragged']['scroll']>rows['default-folded-scrollbar']['scroll']+100
    row=rows['default-folded-scrollbar'];scale=row['pixels_per_point'];x,y,width,height=row['track'];assert width==14
    image=Image.open(Path(runs[0]['evidence'])/'default-folded-scrollbar.png').convert('RGB');cx=round((x+width/2)*scale)
    bright=[]
    for py in range(round(y*scale),min(image.height,round((y+height)*scale))):
        color=image.getpixel((cx,py))
        if min(color)>100 and max(color)-min(color)<20:bright.append(py)
    groups=[]
    for py in bright:
        if not groups or py>groups[-1][-1]+1:groups.append([py])
        else:groups[-1].append(py)
    handle=max(groups,key=len);assert len(handle)>=42*scale,(len(handle),scale)
    result.update(checks_pass=True,scrollbar_track_width_pt=width,visible_handle_length_px=len(handle),visible_handle_length_pt=len(handle)/scale,persistence_verified=True)
    assert result['source_unchanged']
except (AssertionError,KeyError,ValueError,FileNotFoundError) as exc:result['failure']=str(exc)
out.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n');print(json.dumps(result,ensure_ascii=False,indent=2));assert result['checks_pass']
