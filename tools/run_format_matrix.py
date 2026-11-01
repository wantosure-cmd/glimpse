"""Exercise each real file through native browser thumbnail/detail/GPU/seek paths.

Images have screenshot quadrant checks; audio measures the application's WASAPI
session, not microphone or physical audibility. Does not send OS mouse events.
"""
import hashlib
import json
import math
import statistics
import subprocess
import time
import uuid
import sys
import argparse
from pathlib import Path

ROOT=Path(__file__).resolve().parent.parent
BASE=ROOT/'test-media/format-matrix-v1'
EXE=ROOT/'dist/browser-preview/media-browser.exe'
FF=ROOT/'vendor/runtime/ffmpeg.exe'
FP=ROOT/'vendor/runtime/ffprobe.exe'
METER=ROOT/'target/diagnostics/Release/audio-meter.exe'


def sha(p):
    h=hashlib.sha256()
    with p.open('rb') as f:
        while b:=f.read(1024*1024):h.update(b)
    return h.hexdigest()


def screenshot(path, row):
    size=json.loads(subprocess.check_output([str(FP),'-v','error','-show_entries','stream=width,height','-of','json',str(path)]))['streams'][0]
    raw=subprocess.check_output([str(FF),'-v','error','-i',str(path),'-f','rawvideo','-pix_fmt','rgb24','-'],creationflags=subprocess.CREATE_NO_WINDOW)
    w,h=size['width'],size['height']
    assert len(raw)==w*h*3
    rect=row['region'];assert rect
    scale=row['scale'];x0,y0,x1,y1=[v*scale for v in rect]
    def pixel(fx,fy):
        x=max(0,min(w-1,round(x0+(x1-x0)*fx)));y=max(0,min(h-1,round(y0+(y1-y0)*fy)))
        return list(raw[(y*w+x)*3:(y*w+x)*3+3])
    corners=[pixel(x,y) for x,y in [(.2,.2),(.8,.2),(.2,.8),(.8,.8)]]
    samples=[pixel(x/12,y/12) for y in range(1,12) for x in range(1,12)]
    assert max(max(p) for p in samples)-min(min(p) for p in samples)>30,'Uniform/black picture'
    return dict(corners=corners,samples=samples,width=w,height=h,region=rect,scale=scale,sha256=sha(path))


def main():
    global EXE
    parser=argparse.ArgumentParser()
    parser.add_argument('--executable',type=Path,default=EXE)
    parser.add_argument('--reverify',action='store_true')
    parser.add_argument('--motion-probe',action='store_true')
    parser.add_argument('--manifest',type=Path,default=BASE/'manifest.json')
    parser.add_argument('--output',type=Path,default=ROOT/'artifacts/p5-format-matrix.json')
    args=parser.parse_args()
    EXE=args.executable.resolve(strict=True)
    assert (BASE/'.generated-format-fixture').is_file()
    manifest=json.loads(args.manifest.read_text(encoding='utf8'))
    reverify=args.reverify
    previous=None
    if reverify:
        previous=json.loads(args.output.read_text(encoding='utf8'))
        assert previous['finished'] and previous['exe_sha256']==sha(EXE)
        assert previous['fixture_manifest_sha256']==sha(args.manifest)
        old_path=args.output.with_name(args.output.stem+'-initial-verification.json')
        if not old_path.exists():old_path.write_text(json.dumps(previous,ensure_ascii=False,indent=2),encoding='utf8')
    nonce=uuid.uuid4().hex
    out=ROOT/'artifacts/p5-format-ui'/nonce
    if not reverify:out.mkdir(parents=True)
    rows=[]
    for case in manifest['cases']:
        src=ROOT/case['path'];assert src.resolve().is_relative_to(BASE)
        assert sha(src)==case['sha256']
        prior=next((r for r in previous['rows'] if r['name']==case['name']),None) if previous else None
        dest=ROOT/prior['evidence'] if prior else out/case['name']
        assert dest.resolve().is_relative_to(ROOT/'artifacts/p5-format-ui')
        if not prior:dest.mkdir()
        data=ROOT/'target'/('format-smoke-'+nonce)/case['name']
        started=prior['started_utc'] if prior else time.time()
        if prior:
            code=prior['exit_code'];pid=prior['pid'];ended=prior['ended_utc']
        else:
          with (dest/'stdout.log').open('wb') as stdout,(dest/'stderr.log').open('wb') as stderr:
            command=[str(EXE),'--format-smoke',str(dest),'--open',str(src.parent),'--data-dir',str(data),'--quit-after','40000']
            if args.motion_probe:command+=['--format-motion-probe']
            app=subprocess.Popen(command,cwd=ROOT,stdout=stdout,stderr=stderr)
            meter=None
            if case['kind']=='video':
                audio_file=(dest/'audio.json').open('wb')
                meter=subprocess.Popen([str(METER),str(app.pid)],stdout=audio_file,stderr=subprocess.PIPE,creationflags=subprocess.CREATE_NO_WINDOW)
            code=app.wait(timeout=50)
            if meter:
                _,meter_error=meter.communicate(timeout=25);audio_file.close()
                assert meter.returncode==0,meter_error
          pid=app.pid;ended=time.time()
        result=dict(name=case['name'],source=case['path'],source_sha256=sha(src),pid=pid,
                    exit_code=code,started_utc=started,ended_utc=ended,evidence=str(dest.relative_to(ROOT)),passed=False)
        try:
            assert code==0
            report=json.loads((dest/'report.json').read_text(encoding='utf8'));result['diagnostic']=report
            assert (dest/'report.json').stat().st_mtime>=started
            assert report['error'] is None,report['error']
            phases={r['phase']:r for r in report['rows']}
            listed,detail,returned=[phases[k] for k in ['list_thumbnail','detail_fit','returned_list']]
            assert all(not r['failures'] for r in phases.values())
            assert listed['textures'] and returned['textures']
            e=case['expected'];w,h=e['width'],e['height']
            if case['kind']=='video':
                stream=next(s for s in case['ffprobe']['streams'] if s['codec_type']=='video')
                w,h=stream['width'],stream['height']
                n,d=map(float,stream.get('sample_aspect_ratio','1:1').replace('N/A','1:1').split(':'))
                if d:w=round(w*n/d)
                rotation=next((x['rotation'] for x in stream.get('side_data_list',[]) if 'rotation' in x),0)
                if abs(rotation)%180==90:w,h=h,w
            assert (listed['record']['width'],listed['record']['height'])==(w,h),(listed['record'],w,h)
            list_pixels=screenshot(dest/'list.png',listed); detail_pixels=screenshot(dest/'detail.png',detail)
            result['screenshots']=dict(list=list_pixels,detail=detail_pixels)
            if case['kind']=='image':
                zoom=phases['detail_zoom'];assert zoom['zoom']>0 and zoom['textures'] and zoom['region']
                assert any(max(t)>=max(w,h) for t in zoom['textures']),'Full-sized source was not decoded for this small fixture'
                if not case['name'].startswith('heic'):
                    colors=[[220,25,35],[25,200,55],[35,55,220],[225,210,25]]
                    order={1:[0,1,2,3],2:[1,0,3,2],6:[2,0,3,1]}[e['orientation']]
                    for image in [list_pixels,detail_pixels]:
                        for actual,index in zip(image['corners'],order):
                            assert max(abs(a-b) for a,b in zip(actual,colors[index]))<32,(case['name'],actual,colors[index])
                    result['quadrants_match_expected_orientation']=True
            else:
                moving,seek=phases['playing_advanced'],phases['paused_seek']
                if args.motion_probe:
                    motion=phases['playing_pixels']
                    assert motion['player']['time']>detail['player']['time']+.1
                    assert motion['seek_target'] is None and motion['player']['pause']=='no'
                    motion_pixels=screenshot(dest/'playing.png',motion)
                    center=[(p,q) for i,(p,q) in enumerate(zip(detail_pixels['samples'],motion_pixels['samples'])) if 2<=i%11<=8 and 2<=i//11<=8]
                    delta=statistics.mean(abs(a-b) for p,q in center for a,b in zip(p,q))
                    assert delta>1,'Clock advanced but GPU video region stayed frozen before any seek'
                    result.update(visible_playback_sample_mean_delta=delta,visible_playback_motion_verified=True,motion_scope='central7x7 video-region grid before pause/seek',motion_frame=motion_pixels)
                player=detail['player'];assert player['ready'] and player['mute']=='no' and player['vo']=='libmpv'
                stable=sum(sum(abs(a-b) for a,b in zip(p,q))/3<50 for p,q in zip(list_pixels['samples'],detail_pixels['samples']))
                assert stable/len(list_pixels['samples'])>.6,'Poster/detail spatial orientation disagrees'
                assert player['ao'] and player['duration']>e['duration_min']
                normalized=''.join(c for c in player['video'].lower() if c.isalnum())
                aliases={'h264':['h264'],'hevc':['hevc','h265'],'mjpeg':['mjpeg','motionjpeg'],'mpeg4':['mpeg4'],'h263':['h263'],'vp9':['vp9']}
                assert any(c in normalized for c in aliases[e['video_codec']]),player
                actual_audio=player['audio'].lower().replace('_','');assert e['audio_codec'].replace('_','') in actual_audio,(player,e)
                assert moving['player']['time']>player['time']+.1
                assert seek['player']['pause']=='yes' and abs(seek['player']['time']-seek['seek_target'])<.2
                assert seek['seek_target']>29
                seek_pixels=screenshot(dest/'after.png',seek)
                delta=statistics.mean(abs(a-b) for p,q in zip(detail_pixels['samples'],seek_pixels['samples']) for a,b in zip(p,q))
                assert delta>1,'Seek did not produce a different visible frame'
                peaks=json.loads((dest/'audio.json').read_text(encoding='utf8'))
                peak=max(r['peak'] for r in peaks if r['pid']==pid)
                assert peak>.0001,'No per-process audio-session signal'
                result.update(audio_peak=peak,visible_seek_sample_mean_delta=delta,seek_frame=seek_pixels,poster_detail_spatial_patch_matches=stable)
            assert sha(src)==case['sha256']
            result['passed']=True
        except (AssertionError,KeyError,FileNotFoundError,ValueError) as exc:
            result['failure']=str(exc)
        rows.append(result)
        summary=dict(exe_sha256=sha(EXE),fixture_manifest_sha256=sha(args.manifest),verification_mode='Rechecked saved native outputs after correcting human-readable codec-name comparison' if reverify else 'Native execution and verification',scope='Actual native browser, generated known codec files and pinned HEICs; programmatic actions, WASAPI signal and FBO screenshots; not physical listening/input or full HDR calibration',checks_pass=all(r['passed'] for r in rows),finished=len(rows)==len(manifest['cases']),rows=rows)
        args.output.write_text(json.dumps(summary,ensure_ascii=False,indent=2),encoding='utf8')
        print(case['name'], 'PASS' if result['passed'] else 'FAIL '+result.get('failure',''),flush=True)
    print('Passed',sum(r['passed'] for r in rows),'of',len(rows),flush=True)
    if not all(r['passed'] for r in rows):raise SystemExit(1)


if __name__=='__main__':main()
