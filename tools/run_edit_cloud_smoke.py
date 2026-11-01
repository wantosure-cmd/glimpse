"""Native edit/read-only CloudDrive smoke; source media never used as edit fixtures."""
import argparse,hashlib,json,subprocess,time,uuid,shutil,statistics,threading
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
EXE=ROOT/'dist/browser-preview/media-browser.exe'
FF=ROOT/'vendor/runtime/ffmpeg.exe'
FP=ROOT/'vendor/runtime/ffprobe.exe'
def sha(p):
    with p.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()

def sample_audio(app, out, started, state):
    """Keep the existing finite-duration meter covering slow native loads too."""
    rows=[];segments=[]
    try:
        while app.poll() is None:
            number=len(segments);path=out/f'audio-segment-{number:03d}.json'
            offset=(time.monotonic()-started)*1000
            with path.open('wb') as stream,(out/f'audio-segment-{number:03d}-stderr.log').open('wb') as error:
                meter=subprocess.Popen([str(ROOT/'target/diagnostics/Release/audio-meter.exe'),str(app.pid)],stdout=stream,stderr=error,creationflags=subprocess.CREATE_NO_WINDOW)
                code=meter.wait(timeout=30)
            segments.append(dict(file=path.name,pid=meter.pid,exit_code=code,start_after_app_spawn_ms=offset))
            if code!=0:raise RuntimeError(f'Audio meter failed: {path}')
            values=json.loads(path.read_text(encoding='utf-8-sig'))
            rows.extend(dict(value,local_meter_ms=value['ms'],ms=value['ms']+offset) for value in values)
    except Exception as error:
        state['error']=str(error)
    finally:
        state['segments']=segments
        (out/'audio.json').write_text(json.dumps(rows,indent=2),encoding='utf8')
        (out/'audio-sampling.json').write_text(json.dumps(dict(state,scope='Successive original meter processes for this app PID; offsets measured from subprocess creation, not physical audibility'),indent=2),encoding='utf8')
def pixels(path,row):
    dimensions=json.loads(subprocess.check_output([str(FP),'-v','error','-show_entries','stream=width,height','-of','json',str(path)]))['streams'][0]
    w,h=dimensions['width'],dimensions['height'];raw=subprocess.check_output([str(FF),'-v','error','-i',str(path),'-f','rawvideo','-pix_fmt','rgb24','-'],creationflags=subprocess.CREATE_NO_WINDOW)
    rect=row['image_region'] or row['video_region'];assert rect
    scale=row['scale'];x0,y0,x1,y1=[v*scale for v in rect]
    result=[]
    for fx,fy in [(.2,.2),(.8,.2),(.2,.8),(.8,.8)]:
        x=round(x0+(x1-x0)*fx);y=round(y0+(y1-y0)*fy);assert 0<=x<w and 0<=y<h
        result.append(list(raw[(y*w+x)*3:(y*w+x)*3+3]))
    return result

def main():
    p=argparse.ArgumentParser();p.add_argument('--cloud-root',type=Path)
    p.add_argument('--source',type=Path,help='Fixed video in cloud root; requires the explicit-source native diagnostic')
    p.add_argument('--executable',type=Path,default=EXE)
    p.add_argument('--output',type=Path)
    p.add_argument('--controlled-focus',action='store_true',help='Explicit application-input focus context with background negative controls; not physical focus proof')
    a=p.parse_args();nonce=uuid.uuid4().hex
    assert not (a.cloud_root and a.controlled_focus),'Controlled edit focus is not a cloud/OS focus assertion'
    assert not a.source or a.cloud_root,'An explicit source requires a cloud root'
    exe=a.executable.resolve(strict=True)
    out=ROOT/'artifacts'/('cloud-edit-'+nonce);out.mkdir();data=ROOT/'target'/('cloud-edit-data-'+nonce)
    if a.cloud_root:
        mode='cloud';folder=a.cloud_root
        if a.source:
            smallest=a.source.resolve(strict=True)
            assert smallest.parent.samefile(folder) and smallest.suffix.lower()=='.mp4'
        else:
            files=[p for p in folder.iterdir() if p.is_file() and p.suffix.lower()=='.mp4']
            smallest=min(files,key=lambda p:p.stat().st_size)
        before_stat=smallest.stat();before=sha(smallest)
    else:
        mode='edit';folder=ROOT/'target'/('edit-fixture-'+nonce);folder.mkdir();(folder/'.edit-smoke-owned').write_text('generated local copies only')
        for src,dst in [('jpeg/sample.jpg','010.jpg'),('png/sample.png','020.png')]:shutil.copyfile(ROOT/'test-media/format-matrix-v1'/src,folder/dst)
        before={p.name:sha(p) for p in folder.iterdir() if p.is_file()}
    originals={str(p):sha(p) for p in [ROOT/'test-media/format-matrix-v1/jpeg/sample.jpg',ROOT/'test-media/format-matrix-v1/png/sample.png']} if mode=='edit' else {}
    output=a.output or ROOT/'artifacts'/('cloud-'+mode+'-verification.json')
    if output.exists():raise FileExistsError('Use a new output report; keep prior failures')
    (out/'source-before.json').write_text(json.dumps(dict(mode=mode,before=before,source=str(smallest) if a.cloud_root else str(folder)),ensure_ascii=False,indent=2),encoding='utf8')
    with (out/'stdout.log').open('wb') as stdout,(out/'stderr.log').open('wb') as stderr:
        command=[str(exe),'--'+mode+'-smoke',str(out),'--open',str(folder),'--data-dir',str(data),'--quit-after','170000']
        if a.source:
            native_source=str(smallest)
            if native_source.startswith('\\\\') and not native_source.startswith('\\\\?\\'):
                native_source='\\\\?\\UNC\\'+native_source[2:]
            elif not native_source.startswith('\\\\?\\'):
                native_source='\\\\?\\'+native_source
            command.extend(['--cloud-smoke-source',native_source])
        if a.controlled_focus:command.append('--edit-smoke-controlled-focus')
        app_started=time.monotonic();app=subprocess.Popen(command,cwd=ROOT,stdout=stdout,stderr=stderr)
        (out/'running.json').write_text(json.dumps(dict(pid=app.pid,exe_sha256=sha(exe),controlled_focus=a.controlled_focus)),encoding='utf-8')
        audio_thread=None;audio_state={}
        if a.cloud_root:
            audio_thread=threading.Thread(target=sample_audio,args=(app,out,app_started,audio_state))
            audio_thread.start()
        code=app.wait(timeout=185)
        if audio_thread:
            audio_thread.join(timeout=35)
            if audio_thread.is_alive():raise TimeoutError('Audio observation still live; inspect before restarting')
    result=json.loads((out/'report.json').read_text(encoding='utf8'))
    output.write_text(json.dumps(dict(checks_pass=False,verification='Not passed; native and audio evidence retained',exit_code=code,error=result.get('error'),exe_sha256=sha(exe),evidence=str(out),data_dir=str(data),scope=result['scope']),ensure_ascii=False,indent=2),encoding='utf8')
    if code!=0 or result['error'] is not None:
        output.write_text(json.dumps(dict(checks_pass=False,exit_code=code,error=result['error'],exe_sha256=sha(exe),evidence=str(out),focus_mode=result.get('focus_mode'),scope=result['scope']),ensure_ascii=False,indent=2),encoding='utf-8')
    assert code==0 and result['error'] is None,result
    rows={r['phase']:r for r in result['rows']};checks={}
    if mode=='edit':
        assert result['focus_mode']==('controlled_application_input' if a.controlled_focus else 'observed_os_focus')
        observed=result['input_observations']
        checks['recorded_input_context']=observed
        if a.controlled_focus:
            assert result['pointer_mode']=='neutral-no-os-cursor-movement'
            assert rows['background-next-blocked']['detail_id']==rows['before']['detail_id']
            assert rows['background-previous-blocked']['detail_id']==rows['next']['detail_id']
            assert [(r['key'],r['applied_input_focus'],r['controlled']) for r in observed]==[('PageDown',False,True),('PageDown',True,True),('PageUp',False,True),('PageUp',True,True)],observed
            checks['background_next_and_previous_blocked']=True
        else:
            assert [(r['key'],r['applied_input_focus'],r['controlled']) for r in observed]==[('PageDown',True,False),('PageUp',True,False)],observed
            assert all(r['observed_os_focus']==r['applied_input_focus'] for r in observed),observed
        assert rows['before']['detail_id']!=rows['next']['detail_id'];assert '失败 0 张' in rows['saved']['rotation_message']
        assert rows['previous']['detail_id']==rows['before']['detail_id']
        original=pixels(out/'before.png',rows['before']);expected=[original[2],original[0],original[3],original[1]]
        for name in ['rotated-jpeg','rotated-png']:
            actual=pixels(out/(name+'.png'),rows[name]);error=statistics.mean(abs(x-y) for p,q in zip(actual,expected) for x,y in zip(p,q));assert error<15,(name,error,actual,expected);checks[name+'_corner_mae']=error
        for name,magic in [('010.jpg',b'\xff\xd8'),('020.png',b'\x89PNG\r\n\x1a\n')]:
            assert sha(folder/name)!=before[name] and (folder/name).read_bytes().startswith(magic)
        assert {p.name for p in folder.iterdir()}==set(before)
        assert sha(folder/'.edit-smoke-owned')==before['.edit-smoke-owned']
        assert all(sha(Path(p))==digest for p,digest in originals.items())
        checks['formats_preserved_and_originals_unchanged']=True
        settings=json.loads((data/'settings.json').read_text(encoding='utf8'));assert settings['shortcuts']==dict(previous='PageUp',next='PageDown')
        checks['configured_shortcuts_persisted']=True
    else:
        selected=Path(rows['cloud-playing']['record']['path'])
        if a.source:assert Path(result['requested_source']).samefile(smallest)
        same_source=selected.samefile(smallest)
        after_stat=smallest.stat();after_hash=sha(smallest)
        (out/'source-after.json').write_text(json.dumps(dict(selected=str(selected),requested=str(smallest),same_file=same_source,before_sha256=before,after_sha256=after_hash,before_size=before_stat.st_size,after_size=after_stat.st_size,before_modified_ns=before_stat.st_mtime_ns,after_modified_ns=after_stat.st_mtime_ns),ensure_ascii=False,indent=2),encoding='utf8')
        assert same_source,'Native diagnostic selected a different file; preserve this failed run'
        assert before==after_hash and (before_stat.st_size,before_stat.st_mtime_ns)==(after_stat.st_size,after_stat.st_mtime_ns)
        playing=rows['cloud-playing']['player'];seek=rows['cloud-seek']['player'];assert playing['ready'] and playing['mute']=='no' and playing['vo']=='libmpv' and playing['ao']=='wasapi'
        assert abs(seek['time']-min(playing['time']+5,playing['duration']-.1))<1,(playing,seek)
        assert not audio_state.get('error'),audio_state
        audio=json.loads((out/'audio.json').read_text(encoding='utf-8-sig'));peak=max(x.get('peak',0) for x in audio if x['pid']==app.pid);assert peak>0.0001,dict(audio_peak=peak,evidence=str(out/'audio.json'))
        actual=pixels(out/'cloud-playing.png',rows['cloud-playing']);assert max(max(p) for p in actual)-min(min(p) for p in actual)>20
        after=pixels(out/'cloud-seek.png',rows['cloud-seek'])
        difference=statistics.mean(abs(x-y) for u,v in zip(actual,after) for x,y in zip(u,v))
        assert difference>1,'Seek did not produce a changed visible frame'
        checks.update(source_unchanged=True,source_alias_same_file=same_source,audio_peak=peak,audio_segments=audio_state['segments'],first_ready_render_ms=playing['first_ready_render_ms'],video_corners=actual,media_count=rows['list']['media_count'],seek_pixel_mean_difference=difference)
    summary=dict(checks_pass=True,mode=mode,exe_sha256=sha(exe),evidence=str(out),data_dir=str(data),focus_mode=result.get('focus_mode'),checks=checks,scope='Native UI rendering, synthetic configured key events with recorded focus context, actual generated-file save, or read-only mounted playback; not physical keypad/OS focus or cloud writes')
    output.write_text(json.dumps(summary,ensure_ascii=False,indent=2),encoding='utf8');print(json.dumps(summary,ensure_ascii=False,indent=2))
if __name__=='__main__':main()
