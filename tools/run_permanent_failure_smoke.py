"""Explicit destructive diagnostic on newly generated owned fixtures only."""
from pathlib import Path
import subprocess,json,uuid,hashlib
ROOT=Path(__file__).resolve().parent.parent
base=ROOT/'target'/('folder-smoke-owned-partial-'+uuid.uuid4().hex);base.mkdir(parents=True)
assert base.resolve().is_relative_to((ROOT/'target').resolve())
(base/'.permanent-smoke-owned').write_bytes(b'generated media only')
photo=base/'photo.jpg';subprocess.run([str(ROOT/'vendor/runtime/ffmpeg.exe'),'-hide_banner','-loglevel','error','-f','lavfi','-i','testsrc=size=4800x3200','-frames:v','1','-q:v','2',str(photo)],check=True)
for name,count in [('123',5),('123/456',5),('123/456/789',8),('delete-me',8),('zz-last',8)]:
 folder=base/name;folder.mkdir(parents=True,exist_ok=True)
 for i in range(count):(folder/('photo.jpg' if i==0 else str(i)+'.jpg')).write_bytes(photo.read_bytes())
(base/'delete-me/notes.txt').write_bytes(b'generated non-media copy')
before={str(f.relative_to(base)):hashlib.sha256(f.read_bytes()).hexdigest() for f in base.rglob('*') if f.is_file()}
(ROOT/'artifacts/permanent-partial-ui-fixture.json').write_text(json.dumps({'root':str(base),'files':before},ensure_ascii=False,indent=2),encoding='utf-8')
exe=ROOT/'dist/browser-preview/media-browser.exe';output=ROOT/'artifacts/permanent-partial-ui-raw.json'
with (ROOT/'artifacts/permanent-partial-ui-stdout.log').open('wb') as out,(ROOT/'artifacts/permanent-partial-ui-stderr.log').open('wb') as err:
 p=subprocess.run([str(exe),'--folder-smoke',str(output),'--confirm-permanent-failure','--open',str(base),'--data-dir',str(ROOT/'target'/('permanent-ui-data-'+uuid.uuid4().hex))],cwd=ROOT,stdout=out,stderr=err,timeout=80)
r=json.loads(output.read_text(encoding='utf-8'));steps={e['step']:e for e in r['steps']}
result={'checks_pass':False,'exit_code':p.returncode,'fixture':str(base),'exe_sha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'full_acceptance_pass':False,'scope':'real native app with controlled egui pointer events confirming deletion only of marked generated workspace copies; not physical OS mouse input'}
try:
 assert p.returncode==0 and r['error'] is None,r.get('error')
 assert r['exe_sha256']==result['exe_sha256']
 assert steps['confirmation_wait']['prompt_still_open'] and steps['confirmation_wait']['source_still_present']
 assert steps['checkbox_enabled']['enabled']
 partial=steps['permanent_partial_result'];assert 0<partial['remaining']<partial['before'] and partial['locked_survivor']
 batch=partial['result'];assert batch['permanent'] and batch['success_count']==0 and batch['failed_count']==1
 assert batch['items'][0]['may_have_deleted_contents'] and not batch['items'][0]['success']
 refresh=steps['partial_refresh'];assert refresh['files_in_snapshot']==refresh['expected_snapshot_files']
 deleted=[]
 for name,digest in before.items():
  f=base/name
  if f.exists():assert hashlib.sha256(f.read_bytes()).hexdigest()==digest
  else:assert Path(name).parts[0]=='delete-me';deleted.append(name)
 assert deleted
 result.update(checks_pass=True,deleted_generated_files=len(deleted),remaining_in_target=partial['remaining'],batch=batch,surviving_sources_unchanged=True,index_matches_remaining_files=True)
except Exception as e:result['error']=repr(e)
(ROOT/'artifacts/permanent-partial-ui-verification.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8');print(json.dumps(result,ensure_ascii=False,indent=2))
raise SystemExit(0 if result['checks_pass'] else 1)
