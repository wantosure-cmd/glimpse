"""Read-only pixel checks on the generated landscape/portrait fullscreen FBOs."""
from pathlib import Path
import hashlib,json,struct,subprocess
ROOT=Path(__file__).resolve().parents[1]
run=json.loads((ROOT/'artifacts/v0449-detail-ui-verification.json').read_text(encoding='utf8'))
raw=json.loads(Path(run['raw']).read_text(encoding='utf8'))
rows={r['name']:r for r in raw['rows']};checks={}
for name in ('video-fullscreen-landscape','video-fullscreen-portrait'):
    row=rows[name];path=Path(run['work'])/'artifacts'/f'design-{name}.png'
    data=path.read_bytes();w,h=struct.unpack('>II',data[16:24])
    rgb=subprocess.check_output([str(ROOT/'vendor/runtime/ffmpeg.exe'),'-v','error','-i',str(path),
        '-f','rawvideo','-pix_fmt','rgb24','-'],creationflags=subprocess.CREATE_NO_WINDOW)
    x,y,width,height=[v*row['pixels_per_point'] for v in row['video_region']]
    samples=[]
    for rx in (.25,.5,.75):
        for ry in (.35,.5,.65):
            px,py=int(x+width*rx),int(y+height*ry)
            samples.append(list(rgb[(py*w+px)*3:][:3]))
    assert max(max(c) for c in samples)>=80,(name,samples)
    checks[name]={'png_sha256':hashlib.sha256(data).hexdigest(),'nonblack_media_pass':True,'rgb_samples':samples}
result={'checks_pass':True,'exe_sha256':run['exe_sha256'],'scope':'Actual native FBO pixels from generated SDR test charts; not HDR or physical-display calibration','checks':checks}
(ROOT/'artifacts/v0449-fullscreen-pixels.json').write_text(json.dumps(result,indent=2),encoding='utf8')
print(json.dumps({'checks_pass':True,'fullscreen_nonblack_frames':len(checks)}))
