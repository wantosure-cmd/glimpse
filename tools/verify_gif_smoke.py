import argparse,hashlib,json,pathlib,subprocess
root=pathlib.Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser();parser.add_argument('--evidence-dir',type=pathlib.Path,default=root/'artifacts');parser.add_argument('--exe',type=pathlib.Path,default=root/'dist/browser-preview/media-browser.exe');args=parser.parse_args();evidence=args.evidence_dir.resolve(strict=True);exe=args.exe.resolve(strict=True)
runtime=exe.parent/'runtime'
if not runtime.is_dir() and exe.parent in (root/'target/release',root/'target/debug'):
    runtime=root/'vendor/runtime'  # Exactly the native app's source-checkout fallback.
ffmpeg=runtime/'ffmpeg.exe'
pin=next(r for r in json.loads((root/'vendor/media-tools.json').read_text(encoding='utf-8-sig')) if r['component']=='ffmpeg')
with ffmpeg.open('rb') as stream:
    assert ffmpeg.stat().st_size==pin['bytes'] and hashlib.file_digest(stream,'sha256').hexdigest().lower()==pin['sha256'].lower()
def read(name):return json.loads((evidence/name).read_text(encoding='utf-8-sig'))
run=read('p3-gif-run.json')
assert run['exit_code']==0
assert hashlib.sha256(exe.read_bytes()).hexdigest().upper()==run['exe_sha256']
result=read('p3-gif-smoke.json')
assert result['error'] is None,result
completed={r['phase']:r for r in result['completed']}
assert completed[0]['last_sequence']>=8 and not completed[0]['ended'],completed
assert completed[1]['last_sequence']==5 and completed[1]['ended'],completed
assert completed[2]['last_sequence']==2 and completed[2]['ended'],completed
intervals=[]
for phase in (0,1,2):
    frames=[r for r in result['frames'] if r['phase']==phase]
    assert frames[0]['sequence']==0,frames
    assert len({r['texture_id'] for r in frames})==1,frames
    assert max(r['texture_bytes'] for r in frames)==8,frames
    for before,after in zip(frames,frames[1:]):
        assert after['sequence']==before['sequence']+1,(before,after)
        measured=after['elapsed_ms']-before['elapsed_ms']
        expected=before['declared_ms']
        assert abs(measured-expected)<50,(phase,measured,expected)
        intervals.append({'phase':phase,'from_sequence':before['sequence'],'declared_ms':expected,'observed_ms':measured})
colors=[]
for index in range(3):
    path=evidence/f'p3-gif-frame-{index}.png'
    color=subprocess.run([str(ffmpeg),'-hide_banner','-loglevel','error','-i',str(path),'-vf','crop=1:1:iw/2:ih/2','-frames:v','1','-f','rawvideo','-pix_fmt','rgba','pipe:1'],capture_output=True,check=True,timeout=10).stdout
    assert len(color)==4,color
    assert color[index]>240 and all(color[channel]<15 for channel in range(3) if channel!=index),list(color)
    colors.append(list(color))
assert read('p3-gif-hashes-before.json')==read('p3-gif-hashes-after.json')
summary={'checks_pass':True,'scope':result['scope'],'exe_sha256':run['exe_sha256'],'source_gifs_unchanged':3,'frame_center_colors_rgba':colors,'single_texture_per_source':True,'finite_repeat_last_sequence':5,'no_repeat_last_sequence':2,'timing_tolerance_ms':50,'max_timing_error_ms':max(abs(r['observed_ms']-r['declared_ms']) for r in intervals),'timing_intervals':intervals}
(evidence/'p3-gif-verification.json').write_text(json.dumps(summary,indent=2),encoding='utf8')
print(json.dumps({key:value for key,value in summary.items() if key!='timing_intervals'},indent=2))
