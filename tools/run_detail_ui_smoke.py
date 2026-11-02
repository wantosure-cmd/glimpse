"""Generated-only native checks for compact video footer, fullscreen, corners and sticky counts."""
from pathlib import Path
import argparse,hashlib,json,shutil,struct,subprocess,uuid,zlib
ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--exe',type=Path,required=True);a=p.parse_args()
task=ROOT/'target'/('detail-ui-owned-'+uuid.uuid4().hex)
media=task/'media';work=task/'work';media.mkdir(parents=True);work.mkdir()
def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data)&0xffffffff)
def png(path):
    path.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',400,600,8,2,0,0,0))+
        chunk(b'IDAT',zlib.compress((b'\0'+bytes((65,120,185))*400)*600))+chunk(b'IEND',b''))
png(media/'01-image.png');png(media/'04-image.png');(media/'album').mkdir()
for i in range(30):png(media/'album'/f'photo-{i:02}.png')
ffmpeg=ROOT/'vendor/runtime/ffmpeg.exe'
for name,size in [('02-landscape.mov','426x240'),('03-portrait.mov','240x426')]:
    subprocess.run([str(ffmpeg),'-v','error','-f','lavfi','-i',f'testsrc2=size={size}:rate=12',
        '-t','30','-c:v','mpeg4','-q:v','5','-pix_fmt','yuv420p','-an',str(media/name)],check=True,creationflags=subprocess.CREATE_NO_WINDOW)
def identity():return {str(f.relative_to(media)):hashlib.sha256(f.read_bytes()).hexdigest() for f in media.rglob('*') if f.is_file()}
before=identity();exe=a.exe.resolve(strict=True);out=ROOT/'artifacts/v0449-detail-ui-raw.json'
with (task/'stdout.log').open('wb') as stdout,(task/'stderr.log').open('wb') as stderr:
    proc=subprocess.Popen([str(exe),'--detail-ui-smoke',str(out),'--open',str(media),'--data-dir',str(task/'data')],
        cwd=work,stdout=stdout,stderr=stderr)
    code=proc.wait(timeout=75)
raw=json.loads(out.read_text(encoding='utf8'));assert code==0 and raw['checks_pass'],raw
rows={r['name']:r for r in raw['rows']}
compact=rows['detail-compact-footer'];layout=rows['layout-detail-compact-footer']
assert compact['strip'][3]<=30.5 and all(r[3]<=24.5 for r in compact['thumbnails'])
screen=compact['screen'];canvas=layout['detail_canvas']
assert screen[1]+screen[3]-canvas[1]-canvas[3]<=64.5
for name in ('detail-large-arrows','detail-corner-scale200'):
    assert all(r[2]>=64 and r[3]>=88 for r in rows[name]['detail_arrows'])
    screen=rows[name]['screen'];close=rows[name]['detail_close']
    assert abs(close[0]+close[2]-screen[0]-screen[2])<0.1 and abs(close[1]-screen[1])<0.1
for name,aspect in [('video-fullscreen-landscape',426/240),('video-fullscreen-portrait',240/426)]:
    row=rows[name];canvas=rows['layout-'+name]['detail_canvas'];screen=row['screen']
    assert row['video_fullscreen'] and row['os_fullscreen'] is True and row['strip'] is None and row['transport'] is None
    assert all(abs(a-b)<0.1 for a,b in zip(canvas,screen)),(canvas,screen)
    rect=row['video_region'];assert abs(rect[2]/rect[3]-aspect)<0.002,rect
assert rows['video-fullscreen-landscape']['fullscreen_button'] is None
assert not rows['video-fullscreen-landscape']['detail_arrows']
assert rows['video-restored-from-fullscreen']['detail_id']==compact['detail_id']
assert rows['video-fullscreen-landscape']['player_media']==compact['player_media']
assert rows['video-fullscreen-landscape']['time']>=compact['time']-0.2
for name in ('sticky-media-counts','sticky-counts-narrow','sticky-counts-scale200'):
    stats=rows[name]['sticky_stats'];assert (stats['images'],stats['videos'],stats['folders'])==(2,2,1),stats
    r=stats['rect'];s=rows[name]['screen'];assert r[0]>=s[0] and r[0]+r[2]<=s[0]+s[2]+0.1
# The portrait is fully visible and centred, with actual black pillars in the FBO.
row=rows['video-fullscreen-portrait'];picture=work/'artifacts/design-video-fullscreen-portrait.png'
data=picture.read_bytes();width,height=struct.unpack('>II',data[16:24]);scale=row['pixels_per_point']
pixels=subprocess.check_output([str(ffmpeg),'-v','error','-i',str(picture),'-f','rawvideo','-pix_fmt','rgb24','-'],creationflags=subprocess.CREATE_NO_WINDOW)
vx,vy,vw,vh=[v*scale for v in row['video_region']]
for x in (int(vx/2),int((vx+vw+width)/2)):
    y=int(vy+vh*0.65);color=pixels[(y*width+x)*3:][:3];assert max(color)<=4,(x,y,list(color))
assert before==identity()
result={'checks_pass':True,'exe_sha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'work':str(work),'raw':str(out),
        'generated_media_unchanged':True,'source_files':len(before),'video_footer_points':64,'video_filmstrip_max_points':30,
        'arrow_hit_points':[64,88],'corner_clicks_pass':True,'fullscreen_click_escape_f11_pass':True,
        'landscape_and_portrait_aspect_pass':True,'actual_portrait_black_pillars_pass':True,
        'sticky_counts_wide_narrow_ui_scale_200_pass':True,'scope':raw['scope']}
(ROOT/'artifacts/v0449-detail-ui-verification.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf8')
print(json.dumps(result,ensure_ascii=False,indent=2))
