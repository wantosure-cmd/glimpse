"""Controlled native UI diagnostic. Uses only generated, explicitly named fixtures."""
from pathlib import Path
import json,subprocess,hashlib,uuid
ROOT=Path(__file__).resolve().parent.parent
(ROOT/'artifacts').mkdir(exist_ok=True)
record=ROOT/'artifacts/folders-fixture.json'
if not record.exists():
    base=ROOT/'target'/('folder-smoke-owned-'+uuid.uuid4().hex);base.mkdir(parents=True)
    photo=base/'photo.jpg'
    subprocess.run([str(ROOT/'vendor/runtime/ffmpeg.exe'),'-hide_banner','-loglevel','error','-f','lavfi','-i','testsrc=size=4800x3200','-frames:v','1','-q:v','2',str(photo)],check=True)
    for name,count in [('123',5),('123/456',5),('123/456/789',8),('delete-me',8),('zz-last',8)]:
        folder=base/name;folder.mkdir(parents=True,exist_ok=True)
        for i in range(count):(folder/('photo.jpg' if i==0 else str(i)+'.jpg')).write_bytes(photo.read_bytes())
    (base/'delete-me/notes.txt').write_text('generated non-media fixture; recycle only',encoding='utf-8')
    record.write_text(json.dumps({'root':str(base),'files':{str(f.relative_to(base)):hashlib.sha256(f.read_bytes()).hexdigest() for f in base.rglob('*') if f.is_file()}},ensure_ascii=False,indent=2),encoding='utf-8')
fixture=json.loads(record.read_text(encoding='utf-8'))
root=Path(fixture['root']); assert root.is_relative_to(ROOT/'target') and root.name.startswith('folder-smoke-owned-')
exe=ROOT/'dist/browser-preview/media-browser.exe'
output=ROOT/'artifacts/folders-ui-smoke.json'
args=[str(exe),'--folder-smoke',str(output),'--open',str(root),'--data-dir',str(ROOT/'target'/('folders-ui-data-'+uuid.uuid4().hex))]
with (ROOT/'artifacts/folders-ui-stdout.log').open('w') as out,(ROOT/'artifacts/folders-ui-stderr.log').open('w') as err:
    proc=subprocess.run(args,cwd=ROOT,stdout=out,stderr=err,timeout=80)
raw=json.loads(output.read_text(encoding='utf-8')) if output.exists() else {}
steps=raw.get('steps',[])
result={'exe_sha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'exit_code':proc.returncode,'error':raw.get('error','no evidence'),'scope':'controlled program actions and native FBO; not physical OS input','full_acceptance_pass':False}
try:
    assert proc.returncode==0 and raw['error'] is None
    assert raw['exe_sha256']==result['exe_sha256']
    assert [s['step'] for s in steps[:7]]==['progressive','sticky','sticky','sticky','locate','undo','commit']
    assert steps[0]['preview_paints']>0
    assert [len(s['paths']) for s in steps[1:4]]==[2,3,4]
    assert steps[4]['offset']==steps[4]['expected']
    assert all(steps[5][k] for k in ('deduplicated','before_deadline','files_present'))
    assert steps[6]['elapsed_seconds']>=10 and steps[6]['original_undo_folder_exists']
    trash=steps[6]['result']
    assert trash['submitted'] and trash['release_confirmed']
    succeeded=trash['success_count']==1
    assert steps[6]['delete_folder_exists']!=succeeded
    if not succeeded:
        assert steps[6]['confirmation_prompt']
        assert steps[7]['step']=='confirmation_wait' and steps[7]['prompt_still_open'] and steps[7]['source_still_present']
        assert (ROOT/'artifacts/folder-permanent-confirmation.png').exists()
    result['permanent_confirmation_cancel_pass']=not succeeded
    after={}
    for name,digest in fixture['files'].items():
        path=root/name
        if path.exists():
            actual=hashlib.sha256(path.read_bytes()).hexdigest(); assert actual==digest;after[name]=actual
        else: assert succeeded and Path(name).parts[0]=='delete-me'
    result.update(ui_flow_pass=True,system_recycle_pass=succeeded,recycle_result=trash,surviving_sources_unchanged=True,step_count=len(steps))
except Exception as e: result.update(ui_flow_pass=False,verification_error=repr(e))
(ROOT/'artifacts/folders-ui-verification.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps(result,ensure_ascii=False,indent=2))
raise SystemExit(0 if result.get('ui_flow_pass') else 1)
