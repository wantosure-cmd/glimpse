"""Summarize measured P0 evidence. Does not label the full product complete."""
import json, pathlib, struct
root=pathlib.Path(__file__).resolve().parent.parent
artifacts=root/'artifacts'
def read(path):return json.loads(path.read_text(encoding='utf-8-sig'))
rows=[]
for run in read(artifacts/'p0-gpu-runs.json'):
    directory=pathlib.Path(run['evidence'])
    smoke=read(directory/'p0-gpu-smoke.json')['steps']
    by_step={r['step']:r for r in smoke}
    audio=read(directory/'audio-peaks.json')
    def peak(lo,hi):return max((r['peak'] for r in audio if lo<r['ms']<hi),default=None)
    probe_name={'mp4':'mp4','mov':'mov','avi':'avi','3gp':'3gp'}[run['file'].rsplit('.',1)[-1]]
    probe=read(artifacts/f'p0-probe-{probe_name}.json')
    screenshots={}
    for name in ['p0-preview.png','p0-detail.png','p0-preview-return.png']:
        data=(directory/name).read_bytes()
        if data[:8]!=b'\x89PNG\r\n\x1a\n':raise ValueError(f'Invalid screenshot {name}')
        screenshots[name]=dict(width=struct.unpack('>I',data[16:20])[0],height=struct.unpack('>I',data[20:24])[0],bytes=len(data))
    result=dict(file=run['file'],exit_code=run['exit_code'],video_codec=probe['video_codec'],audio_codec=probe['audio_codec'],
                seek_results=probe['seeks'],preview_mute=by_step[1]['mute'],detail_mute=by_step[5]['mute'],return_mute=by_step[8]['mute'],
                output=by_step[5]['video_output'],hardware_decode=by_step[5]['hwdec'],audio_output=by_step[5]['audio_device'],
                preview_audio_peak=peak(3500,5500),detail_audio_peak=peak(8500,11500),return_audio_peak=peak(16500,18500),screenshots=screenshots)
    result['checks_pass']=bool(run['exit_code']==0 and all(s['pass'] for s in probe['seeks']) and result['preview_mute']=='yes' and result['detail_mute']=='no' and result['return_mute']=='yes' and result['output']=='libmpv' and result['preview_audio_peak']==0 and result['detail_audio_peak'] and result['detail_audio_peak']>0.001 and result['return_audio_peak']==0)
    rows.append(result)
summary=dict(scope='P0 programmatic render/seek/audio evidence, not complete product or physical speaker test',formats=rows,
             startup=read(artifacts/'p0-startup-summary.json'),idle=read(artifacts/'p0-idle.json'))
(artifacts/'p0-evidence-summary.json').write_text(json.dumps(summary,ensure_ascii=False,indent=2),encoding='utf8')
print(json.dumps(summary,ensure_ascii=False,indent=2))
if len(rows)!=4 or not all(r['checks_pass'] for r in rows):raise SystemExit(1)
